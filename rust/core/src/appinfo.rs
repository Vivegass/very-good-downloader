//! Общие сведения о программе: имя, папки данных, где искать yt-dlp / ffmpeg / deno.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub const APP_NAME: &str = "Very Good Downloader";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Не показывать чёрные окна консоли у дочерних процессов (ffmpeg, yt-dlp).
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------- пути, заданные при запуске (Android) ----------

/// Как запускать внешний инструмент: программа и аргументы перед своими.
/// На Android yt-dlp — это `libpython.so <папка>/yt-dlp --ffmpeg-location … --js-runtimes quickjs:…`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tool {
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

/// Пути и окружение, которые программа задаёт при запуске (`init`). На Windows не задаются — всё ищется
/// как раньше (`%APPDATA%`, `bin\` рядом с exe, PATH); Android передаёт папку данных приложения,
/// yt-dlp / ffmpeg из nativeLibraryDir и переменные для Python (`LD_LIBRARY_PATH`, `PYTHONHOME`, …).
#[derive(Clone, Debug, Default)]
pub struct Paths {
    /// папка настроек, тем, списков (вместо `%APPDATA%\VeryGoodDownloader`)
    pub data_dir: Option<PathBuf>,
    /// инструменты по имени: "yt-dlp", "ffmpeg"
    pub tools: Vec<(String, Tool)>,
    /// переменные окружения для всех дочерних процессов
    pub env: Vec<(String, String)>,
    /// сколько ГБ оставлять свободными (по умолчанию `downloader::MIN_FREE_GB`)
    pub min_free_gb: Option<f64>,
}

static PATHS: OnceLock<Paths> = OnceLock::new();

/// Задать пути один раз при запуске, до первой загрузки. Повторный вызов ничего не меняет (false).
pub fn init(paths: Paths) -> bool {
    PATHS.set(paths).is_ok()
}

fn paths() -> Option<&'static Paths> {
    PATHS.get()
}

fn configured_tool(name: &str) -> Option<&'static Tool> {
    paths()?.tools.iter().find(|(n, _)| n == name).map(|(_, t)| t)
}

/// Инструмент задан в `init` (Android), а не найден рядом с программой.
pub fn tool_configured(name: &str) -> bool {
    configured_tool(name).is_some()
}

/// Свободного места не меньше, ГБ (задано в `init`).
pub fn min_free_gb() -> Option<f64> {
    paths().and_then(|p| p.min_free_gb)
}

/// `%APPDATA%\VeryGoodDownloader` — те же settings.json и topics.json, что у Python-версии.
/// На Android — папка, переданная в `init`.
pub fn data_dir() -> PathBuf {
    if let Some(d) = paths().and_then(|p| p.data_dir.clone()) {
        return d;
    }
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

/// Имя исполняемого файла: `yt-dlp.exe` на Windows, `yt-dlp` на Linux/Android.
fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Полный путь к инструменту: заданный в `init`, свои папки, затем PATH.
pub fn tool(name: &str) -> PathBuf {
    if let Some(t) = configured_tool(name) {
        return t.program.clone();
    }
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    let exe = exe_name(name);
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
/// Если `program` — инструмент, заданный в `init`, к команде добавляются его аргументы-приставка
/// (`libpython.so …/yt-dlp …`) и переменные окружения из `init`.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    command_with(paths(), program.as_ref())
}

fn command_with(paths: Option<&Paths>, program: &std::ffi::OsStr) -> Command {
    let tool = paths.and_then(|p| p.tools.iter().map(|(_, t)| t).find(|t| t.program.as_os_str() == program));
    let mut c = Command::new(program);
    if let Some(t) = tool {
        c.args(&t.prefix);
    }
    c.env("PATH", child_path()).env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
    if let Some(p) = paths {
        for (k, v) in &p.env {
            c.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    // Своя группа процессов: отмена гасит и детей (python -> ffmpeg), см. downloader::kill_tree.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_tool_gets_prefix_and_env() {
        let p = Paths {
            tools: vec![(
                "yt-dlp".into(),
                Tool { program: PathBuf::from("/app/lib/libpython.so"), prefix: vec!["/files/yt-dlp".into(), "--ffmpeg-location".into(), "/app/lib/libffmpeg.so".into()] },
            )],
            env: vec![("PYTHONHOME".into(), "/files/python/usr".into())],
            ..Default::default()
        };
        let mut c = command_with(Some(&p), std::ffi::OsStr::new("/app/lib/libpython.so"));
        c.arg("--version");
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["/files/yt-dlp", "--ffmpeg-location", "/app/lib/libffmpeg.so", "--version"]);
        assert!(c.get_envs().any(|(k, v)| k == "PYTHONHOME" && v == Some(std::ffi::OsStr::new("/files/python/usr"))));
        // другой инструмент — без приставки
        let c = command_with(Some(&p), std::ffi::OsStr::new("ffmpeg"));
        assert_eq!(c.get_args().count(), 0);
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(vtuple("2026.08.19") == vtuple("2026.8.19"));
        assert!(vtuple("2026.10.1") > vtuple("2026.9.30"));
    }
}
