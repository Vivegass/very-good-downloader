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
