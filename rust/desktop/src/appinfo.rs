//! Сведения о программе, нужные только версии для Windows: браузеры и расширение, уведомления,
//! Проводник, автозапуск. Общее (папки, поиск инструментов, запуск процессов) — в `vgd_core::appinfo`.

pub use vgd_core::appinfo::*;

use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::sync::OnceLock;

/// Папка с расширением для браузера: рядом с программой (установка) или в исходниках (разработка).
pub fn extension_dir() -> Option<PathBuf> {
    app_dir().ancestors().take(5).map(|d| d.join("extension")).find(|d| d.join("manifest.json").is_file())
}

/// Установленные браузеры на движке Chromium: (название, exe, страница расширений).
pub fn browsers() -> Vec<(&'static str, PathBuf, &'static str)> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
    let (la, pf, pf86) = (env("LOCALAPPDATA"), env("ProgramFiles"), env("ProgramFiles(x86)"));
    let candidates: [(&str, Vec<PathBuf>, &str); 3] = [
        (
            "Яндекс.Браузер",
            vec![
                la.join(r"Yandex\YandexBrowser\Application\browser.exe"),
                pf.join(r"Yandex\YandexBrowser\Application\browser.exe"),
                pf86.join(r"Yandex\YandexBrowser\Application\browser.exe"),
            ],
            "browser://extensions",
        ),
        (
            "Chrome",
            vec![
                pf.join(r"Google\Chrome\Application\chrome.exe"),
                pf86.join(r"Google\Chrome\Application\chrome.exe"),
                la.join(r"Google\Chrome\Application\chrome.exe"),
            ],
            "chrome://extensions",
        ),
        (
            "Edge",
            vec![pf86.join(r"Microsoft\Edge\Application\msedge.exe"), pf.join(r"Microsoft\Edge\Application\msedge.exe")],
            "edge://extensions",
        ),
    ];
    candidates
        .into_iter()
        .filter_map(|(name, paths, url)| paths.into_iter().find(|p| p.is_file()).map(|p| (name, p, url)))
        .collect()
}

/// Firefox, если установлен.
pub fn firefox() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
    [
        env("ProgramFiles").join(r"Mozilla Firefox\firefox.exe"),
        env("ProgramFiles(x86)").join(r"Mozilla Firefox\firefox.exe"),
        env("LOCALAPPDATA").join(r"Mozilla Firefox\firefox.exe"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Подписанное Mozilla расширение для Firefox: рядом с программой (установка)
/// или последнее собранное в `rust\_dl\firefox` (разработка).
pub fn firefox_xpi() -> Option<PathBuf> {
    let app = app_dir();
    if let Some(p) = app.ancestors().take(5).map(|d| d.join("extension-firefox.xpi")).find(|p| p.is_file()) {
        return Some(p);
    }
    app.ancestors()
        .take(5)
        .map(|d| d.join(r"_dl\firefox"))
        .find(|d| d.is_dir())
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "xpi"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Скопировать расширение в постоянную папку `%APPDATA%\VeryGoodDownloader\extension`:
/// она не меняется при обновлении программы, и браузер не теряет расширение.
pub fn install_extension_files() -> Result<PathBuf, String> {
    let src = extension_dir().ok_or("папка расширения не найдена рядом с программой")?;
    let dst = data_dir().join("extension");
    fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            let p = e.path();
            if p.is_dir() {
                copy_dir(&p, &to.join(e.file_name()))?;
            } else {
                std::fs::copy(&p, to.join(e.file_name()))?;
            }
        }
        Ok(())
    }
    if src != dst {
        copy_dir(&src, &dst).map_err(|e| format!("не удалось скопировать расширение: {e}"))?;
    }
    Ok(dst)
}

/// Имя, под которым Windows показывает наши уведомления (иначе они подписаны «Windows PowerShell»).
const TOAST_APP_ID: &str = "VeryGoodDownloader.App";

/// Записать в HKCU название и значок для уведомлений. Без этой записи Windows не покажет тост с нашим именем.
#[cfg(windows)]
fn register_toast_app() -> bool {
    static DONE: OnceLock<bool> = OnceLock::new();
    *DONE.get_or_init(|| {
        use winreg::enums::HKEY_CURRENT_USER;
        let Ok((key, _)) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"Software\Classes\AppUserModelId\{TOAST_APP_ID}")) else {
            return false;
        };
        if key.set_value("DisplayName", &APP_NAME).is_err() {
            return false;
        }
        let icon = data_dir().join("app.png");
        if !icon.is_file() {
            let _ = std::fs::create_dir_all(data_dir());
            let _ = std::fs::write(&icon, include_bytes!("../../app.png"));
        }
        if icon.is_file() {
            let _ = key.set_value("IconUri", &icon.display().to_string());
        }
        true
    })
}

#[cfg(not(windows))]
fn register_toast_app() -> bool {
    false
}

/// Уведомление Windows («тост»). Щелчок по нему открывает папку `open`.
pub fn toast(title: &str, text: &str, open: &Path) {
    let uri = format!("file:///{}", open.display().to_string().replace('\\', "/").replace(' ', "%20"));
    toast_uri(title, text, &uri);
}

/// Уведомление Windows, щелчок по которому открывает ссылку (`https://…`, `vgd://show`).
pub fn toast_uri(title: &str, text: &str, uri: &str) {
    // Не удалось записать своё имя — показываем от имени PowerShell, как раньше: лучше так, чем никак.
    let app_id = if register_toast_app() { TOAST_APP_ID } else { r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe" };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let xml = format!(
        "<toast activationType=\"protocol\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        esc(uri),
        esc(title),
        esc(text)
    );
    // XML передаём через переменную окружения — так не нужно экранировать кавычки для PowerShell.
    let script = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
        [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null; \
        $x = New-Object Windows.Data.Xml.Dom.XmlDocument; $x.LoadXml($env:VGD_TOAST); \
        $t = New-Object Windows.UI.Notifications.ToastNotification $x; \
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($env:VGD_TOAST_APP).Show($t)";
    let _ = command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .env("VGD_TOAST", xml)
        .env("VGD_TOAST_APP", app_id)
        .spawn();
}

/// Открыть ссылку в браузере по умолчанию.
pub fn open_url(url: &str) {
    if url.starts_with("https://") {
        let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
}

/// Открыть файл или папку в Проводнике / программе по умолчанию.
pub fn open_path(path: &Path) {
    let _ = Command::new("explorer").arg(path).spawn();
}

/// Показать файл в Проводнике (выделенным).
pub fn reveal(path: &Path) {
    let mut c = Command::new("explorer");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(format!("/select,\"{}\"", path.display()));
    }
    #[cfg(not(windows))]
    c.arg(path);
    let _ = c.spawn();
}

/// Имя записи в `HKCU\...\Run`: запуск в фоне при входе в Windows.
const AUTOSTART_KEY: &str = "VeryGoodDownloader";

/// Включён ли запуск в фоне при входе в Windows (запись есть — неважно, кем поставлена:
/// программой или установщиком).
#[cfg(windows)]
pub fn autostart() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .and_then(|k| k.get_value::<String, _>(AUTOSTART_KEY))
        .is_ok()
}

#[cfg(not(windows))]
pub fn autostart() -> bool {
    false
}

/// Запуск в фоне при входе в Windows: `"…\Very Good Downloader.exe" --background`.
#[cfg(windows)]
pub fn set_autostart(on: bool, exe: &Path) {
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok((key, _)) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else { return };
    if on {
        let _ = key.set_value(AUTOSTART_KEY, &format!("\"{}\" --background", exe.display()));
    } else {
        let _ = key.delete_value(AUTOSTART_KEY);
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_on: bool, _exe: &Path) {}

/// Запись автозапуска ведёт на программу, которой больше нет (переустановили в другую папку), —
/// перевести её на этот exe.
#[cfg(windows)]
pub fn fix_autostart(exe: &Path) {
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok(v) = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .and_then(|k| k.get_value::<String, _>(AUTOSTART_KEY))
    else {
        return;
    };
    let old = v.trim_start_matches('"').split('"').next().unwrap_or("");
    if !Path::new(old).is_file() {
        set_autostart(true, exe);
    }
}

#[cfg(not(windows))]
pub fn fix_autostart(_exe: &Path) {}
