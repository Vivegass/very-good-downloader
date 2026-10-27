//! Кнопки «скачать» в приложении Spotify для Windows: расширение Spicetify `vgd.js`.
//!
//! Установка: свежий Spicetify (если его нет или он старый) → `vgd.js` в его папку расширений →
//! `spicetify config spotify_path … extensions vgd.js` → `spicetify backup apply`.
//! SpotX и Spicetify совместимы: SpotX правит Spotify первым, Spicetify — поверх.
//!
//! Почему у людей Spicetify «слетает»: Spotify обновляется и ставит чистый `Apps\xpui.spa`.
//! Поэтому программа проверяет это при каждом запуске окна и при входе в Windows (`--spotify-repair`)
//! и, если кнопки пропали, применяет Spicetify заново — пока Spotify закрыт.

use std::path::{Path, PathBuf};

use crate::appinfo::{command, vtuple};

/// Само расширение (вшито в программу).
pub const EXTENSION_JS: &str = include_str!("../../spicetify/vgd.js");
const EXT_NAME: &str = "vgd.js";

fn env_dir(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_default()
}

/// Папка обычной (не Microsoft Store) версии Spotify.
pub fn spotify_dir() -> Option<PathBuf> {
    let d = env_dir("APPDATA").join("Spotify");
    d.join("Spotify.exe").exists().then_some(d)
}

/// Spotify из Microsoft Store: Spicetify его не поддерживает.
pub fn store_spotify() -> bool {
    std::fs::read_dir(env_dir("LOCALAPPDATA").join("Packages"))
        .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("SpotifyAB.SpotifyMusic")))
        .unwrap_or(false)
}

pub fn spicetify_exe() -> Option<PathBuf> {
    let p = env_dir("LOCALAPPDATA").join("spicetify").join("spicetify.exe");
    p.exists().then_some(p)
}

/// Папка настроек Spicetify (`%APPDATA%\spicetify`).
fn config_dir() -> PathBuf {
    env_dir("APPDATA").join("spicetify")
}

pub fn spotify_running() -> bool {
    command("tasklist")
        .args(["/FI", "IMAGENAME eq Spotify.exe", "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Spotify.exe"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    /// Spotify не найден
    NoSpotify,
    /// Spotify из Microsoft Store
    Store,
    /// кнопок нет (Spicetify не применён или Spotify обновился)
    Off,
    /// кнопки на месте
    On,
}

/// Есть ли кнопки в Spotify сейчас: Spicetify распаковал интерфейс в `Apps\xpui` и в нём наше расширение.
pub fn status() -> Status {
    let Some(dir) = spotify_dir() else {
        return if store_spotify() { Status::Store } else { Status::NoSpotify };
    };
    let xpui = dir.join("Apps").join("xpui");
    let patched = xpui.join("index.html").exists() && !dir.join("Apps").join("xpui.spa").exists();
    if patched && xpui.join("extensions").join(EXT_NAME).exists() {
        Status::On
    } else {
        Status::Off
    }
}

fn run(exe: &Path, args: &[&str], log: &dyn Fn(String)) -> Result<String, String> {
    let o = command(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("не удалось запустить spicetify: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    // Spicetify раскрашивает вывод — убираем escape-коды.
    let clean: String = strip_ansi(&text);
    for l in clean.lines().map(str::trim).filter(|l| !l.is_empty()) {
        log(format!("spicetify: {l}"));
    }
    if o.status.success() && !clean.contains("error") {
        Ok(clean)
    } else {
        Err(clean.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ошибка").trim().to_string())
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            while let Some(&n) = it.peek() {
                it.next();
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Последний Spicetify с GitHub: (версия, ссылка на zip для Windows x64).
fn latest_spicetify() -> Result<(String, String), String> {
    let o = command("curl.exe")
        .args(["-s", "-S", "-L", "--max-time", "30", "-H", "User-Agent: VeryGoodDownloader", "https://api.github.com/repos/spicetify/cli/releases/latest"])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).map_err(|_| "GitHub не ответил".to_string())?;
    let ver = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
    let url = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["browser_download_url"].as_str())
        .find(|u| u.ends_with("windows-x64.zip"))
        .ok_or("на GitHub нет сборки Spicetify для Windows")?
        .to_string();
    Ok((ver, url))
}

fn installed_version(exe: &Path) -> Option<String> {
    let o = command(exe).arg("-v").output().ok()?;
    let v = strip_ansi(&String::from_utf8_lossy(&o.stdout)).trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Поставить или обновить Spicetify (только сам spicetify.exe и его файлы — в `%LOCALAPPDATA%\spicetify`).
fn ensure_spicetify(log: &dyn Fn(String)) -> Result<PathBuf, String> {
    let have = spicetify_exe().and_then(|e| installed_version(&e).map(|v| (e, v)));
    let latest = latest_spicetify();
    match (&have, &latest) {
        (Some((exe, v)), Ok((lv, _))) if vtuple(v) >= vtuple(lv) => return Ok(exe.clone()),
        (Some((exe, v)), Err(e)) => {
            log(format!("Не узнал, есть ли новый Spicetify ({e}) — оставляю {v}."));
            return Ok(exe.clone());
        }
        _ => {}
    }
    let (ver, url) = latest?;
    log(match &have {
        Some((_, v)) => format!("Обновляю Spicetify {v} → {ver} (новые версии Spotify нужен свежий Spicetify)…"),
        None => format!("Ставлю Spicetify {ver}…"),
    });
    let dir = env_dir("LOCALAPPDATA").join("spicetify");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip = std::env::temp_dir().join(format!("spicetify-{ver}.zip"));
    let ok = command("curl.exe")
        .args(["-s", "-S", "-L", "--max-time", "300", "-o"])
        .arg(&zip)
        .arg(&url)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err("не удалось скачать Spicetify".into());
    }
    // tar в Windows 10+ умеет zip.
    // Именно tar из Windows (bsdtar): GNU tar из Git принимает «C:» за сетевой адрес.
    let tar = env_dir("SystemRoot").join("System32").join("tar.exe");
    let ok = command(if tar.exists() { tar } else { PathBuf::from("tar") }).arg("-xf").arg(&zip).arg("-C").arg(&dir).status().is_ok_and(|s| s.success());
    let _ = std::fs::remove_file(&zip);
    if !ok {
        return Err("не удалось распаковать Spicetify".into());
    }
    spicetify_exe().ok_or_else(|| "spicetify.exe не появился".into())
}

/// Значение ключа в config-xpui.ini.
fn ini_value(ini: &str, key: &str) -> Option<String> {
    ini.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// Резервная копия Spicetify от другой версии / другой папки Spotify (например, переносной) мешает
/// `backup`: убираем её в сторону (не удаляем) и сбрасываем запись о ней.
fn reset_foreign_backup(spotify: &Path, log: &dyn Fn(String)) {
    let ini_path = config_dir().join("config-xpui.ini");
    let Ok(ini) = std::fs::read_to_string(&ini_path) else { return };
    let path_same = ini_value(&ini, "spotify_path").is_some_and(|p| Path::new(&p) == spotify);
    let backup_ver = ini_value(&ini, "version").unwrap_or_default();
    // Чистый Spotify (xpui.spa на месте) + старая запись о копии = копия устарела (Spotify обновился).
    let fresh = spotify.join("Apps").join("xpui.spa").exists();
    if backup_ver.is_empty() || (path_same && !fresh) {
        return;
    }
    let old = config_dir().join("Backup");
    if old.exists() {
        let to = config_dir().join(format!("Backup-{}", backup_ver.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_")));
        let _ = std::fs::remove_dir_all(&to);
        if std::fs::rename(&old, &to).is_ok() {
            log(format!("Старую копию Spicetify (Spotify {backup_ver}) отложил в {}", to.display()));
        }
    }
    let fixed: Vec<String> = ini
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if k.trim() == "version" || k.trim() == "with" => format!("{} = ", k.trim_end()),
            _ => l.to_string(),
        })
        .collect();
    let _ = std::fs::write(&ini_path, fixed.join("\r\n"));
}

/// Поставить кнопки в Spotify (или вернуть их после обновления Spotify).
/// `restart` — Spicetify сам перезапустит Spotify, если он открыт.
pub fn install(restart: bool, log: &dyn Fn(String)) -> Result<(), String> {
    let spotify = match status() {
        Status::NoSpotify => return Err("Spotify для Windows не найден (нужна версия с spotify.com, не из Microsoft Store)".into()),
        Status::Store => return Err("Spotify из Microsoft Store не поддерживается — поставьте версию с spotify.com".into()),
        _ => spotify_dir().unwrap(),
    };
    let exe = ensure_spicetify(log)?;
    // Первый запуск создаёт config-xpui.ini.
    if !config_dir().join("config-xpui.ini").exists() {
        let _ = run(&exe, &["-c"], log);
    }
    let ext_dir = config_dir().join("Extensions");
    std::fs::create_dir_all(&ext_dir).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join(EXT_NAME), EXTENSION_JS).map_err(|e| format!("не записал {EXT_NAME}: {e}"))?;
    reset_foreign_backup(&spotify, log);
    let prefs = spotify.join("prefs");
    let spotify_s = spotify.to_string_lossy().into_owned();
    let mut cfg = vec!["config", "spotify_path", spotify_s.as_str()];
    let prefs_s = prefs.to_string_lossy().into_owned();
    if prefs.exists() {
        cfg.extend(["prefs_path", prefs_s.as_str()]);
    }
    run(&exe, &cfg, log)?;
    // Добавить vgd.js к уже включённым расширениям (не трогая остальные).
    let ini = std::fs::read_to_string(config_dir().join("config-xpui.ini")).unwrap_or_default();
    if !ini_value(&ini, "extensions").unwrap_or_default().split('|').any(|e| e.trim() == EXT_NAME) {
        run(&exe, &["config", "extensions", EXT_NAME], log)?;
    }
    // Spicetify уже применён к этому Spotify — достаточно `apply`; чистый Spotify — сначала `backup`.
    let applied = !spotify.join("Apps").join("xpui.spa").exists() && spotify.join("Apps").join("xpui").is_dir();
    let mut args = if applied { vec!["apply"] } else { vec!["backup", "apply"] };
    if !restart {
        args.push("--no-restart");
    }
    run(&exe, &args, log).map_err(|e| format!("spicetify apply: {e}"))?;
    if status() == Status::On {
        log("Кнопки «скачать» в Spotify готовы.".into());
        Ok(())
    } else {
        Err("Spicetify отработал, но кнопок в Spotify не видно — подробности в журнале".into())
    }
}

/// Тихая проверка: кнопки пропали (Spotify обновился) и Spotify закрыт — вернуть их.
/// Открытый Spotify не трогаем: вернём при следующей проверке.
pub fn repair(log: &dyn Fn(String)) -> Option<Result<(), String>> {
    if status() != Status::Off || spotify_running() {
        return None;
    }
    log("Spotify обновился — возвращаю кнопки «скачать»…".into());
    Some(install(false, log))
}

/// Проверка при входе в Windows: `Very Good Downloader.exe --spotify-repair` (без окна).
#[cfg(windows)]
pub fn set_autorepair(on: bool, exe: &Path) {
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok((key, _)) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else { return };
    if on {
        let _ = key.set_value("VeryGoodDownloader-Spotify", &format!("\"{}\" --spotify-repair", exe.display()));
    } else {
        let _ = key.delete_value("VeryGoodDownloader-Spotify");
    }
}

#[cfg(not(windows))]
pub fn set_autorepair(_on: bool, _exe: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_and_ansi() {
        let ini = "[Setting]\nspotify_path           = C:\\Spotify\nextensions            = a.js|vgd.js\n[Backup]\nversion = 1.1.96\n";
        assert_eq!(ini_value(ini, "spotify_path").as_deref(), Some("C:\\Spotify"));
        assert_eq!(ini_value(ini, "version").as_deref(), Some("1.1.96"));
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m done"), "ok done");
        assert!(EXTENSION_JS.contains("ContextMenu"));
    }
}
