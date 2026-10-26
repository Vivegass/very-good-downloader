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

/// Уведомление Windows («тост»). Щелчок по нему открывает папку `open`.
pub fn toast(title: &str, text: &str, open: &Path) {
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
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe').Show($t)";
    let _ = command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .env("VGD_TOAST", xml)
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
