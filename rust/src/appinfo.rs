//! Общие сведения о программе: имя, папки данных, где искать yt-dlp / ffmpeg / deno.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub const APP_NAME: &str = "Very Good Downloader";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Не показывать чёрные окна консоли у дочерних процессов (ffmpeg, yt-dlp).
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `%APPDATA%\VeryGoodDownloader` — те же settings.json и topics.json, что у Python-версии.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(app_dir);
    base.join("VeryGoodDownloader")
}

/// Папка, где лежит запущенный .exe.
pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Папки с ffmpeg, deno и yt-dlp, по порядку поиска.
/// `%APPDATA%\...\bin` — сюда кладётся обновлённый yt-dlp; `bin\` рядом с программой — из установщика;
/// `rust\bin` — для запуска из `target\release` во время разработки.
pub fn tool_dirs() -> Vec<PathBuf> {
    let app = app_dir();
    let mut dirs = vec![data_dir().join("bin"), app.join("bin")];
    for up in app.ancestors().skip(1).take(3) {
        dirs.push(up.join("bin"));
    }
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

fn find_in(dirs: &[PathBuf], exe: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

fn which(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file())
}

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
            let _ = std::fs::write(&icon, include_bytes!("../app.png"));
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
    // Не удалось записать своё имя — показываем от имени PowerShell, как раньше: лучше так, чем никак.
    let app_id = if register_toast_app() { TOAST_APP_ID } else { r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe" };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let uri = format!("file:///{}", open.display().to_string().replace('\\', "/").replace(' ', "%20"));
    let xml = format!(
        "<toast activationType=\"protocol\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        esc(&uri),
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

/// Полный путь к инструменту: свои папки, затем PATH.
pub fn tool(name: &str) -> PathBuf {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    let exe = format!("{name}.exe");
    let dirs = DIRS.get_or_init(tool_dirs);
    find_in(dirs, &exe).or_else(|| which(&exe)).unwrap_or_else(|| PathBuf::from(exe))
}

/// PATH для дочерних процессов: yt-dlp ищет там ffmpeg и deno.
pub fn child_path() -> std::ffi::OsString {
    let mut parts: Vec<PathBuf> = tool_dirs();
    if let Some(p) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(parts).unwrap_or_default()
}

/// Команда без окна консоли и с UTF-8 выводом у Python (yt-dlp).
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(program);
    c.env("PATH", child_path()).env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// «2026.08.19» и «2026.8.19» — одна версия: сравниваем числами.
pub fn vtuple(v: &str) -> Vec<u64> {
    v.trim().split('.').filter_map(|x| x.parse().ok()).collect()
}

/// Служебные файлы и папки (архив скачанного, индекс каналов, `.parts`) — скрытые в Проводнике:
/// нужны программе, а не человеку.
pub fn hide(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetFileAttributesW(name: *const u16) -> u32;
            fn SetFileAttributesW(name: *const u16, attrs: u32) -> i32;
        }
        const HIDDEN: u32 = 0x2;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            let a = GetFileAttributesW(wide.as_ptr());
            if a != u32::MAX && a & HIDDEN == 0 {
                SetFileAttributesW(wide.as_ptr(), a | HIDDEN);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = path;
}

/// Дописать строку в служебный файл (и скрыть его).
pub fn append_hidden(path: &Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
    hide(path);
}

/// Записать служебный файл целиком. Скрытый файл Windows не даёт пересоздать (`fs::write`),
/// поэтому открываем существующий и обрезаем.
pub fn write_hidden(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(path)?;
    f.set_len(0)?;
    f.write_all(data)?;
    drop(f);
    hide(path);
    Ok(())
}

/// Создать служебную папку (скрытую).
pub fn hidden_dir(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    hide(path);
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
