//! Прогресс фоновой загрузки: читает download_log.txt и показывает, что происходит.
//! Запуск: `vgd --watch [папка загрузок]`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::downloader::free_gb;

const ANSI_CLEAR: &str = "\x1b[2J\x1b[H";

pub fn watch(root: &Path) -> ! {
    enable_ansi();
    let log = root.join("download_log.txt");
    loop {
        let screen = render(root, &std::fs::read(&log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default());
        print!("{ANSI_CLEAR}{screen}\n");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Последняя строка прогресса: «[3/25 ]сообщение   12.3%   1.2 МБ/с   0:15».
fn parse_progress(line: &str) -> Option<(String, String, f64, String, String)> {
    let pct_end = line.find('%')?;
    let before = line[..pct_end].trim_end();
    let num_start = before.rfind(' ').map_or(0, |i| i + 1);
    let pct: f64 = before[num_start..].parse().ok()?;
    let mut head = before[..num_start].trim().to_string();
    let mut item = String::new();
    if let Some((first, rest)) = head.split_once(' ') {
        if first.contains('/') && first.chars().all(|c| c.is_ascii_digit() || c == '/') {
            item = first.to_string();
            head = rest.trim().to_string();
        }
    }
    let tail: Vec<&str> = line[pct_end + 1..].split_whitespace().collect();
    let (speed, eta) = match tail.as_slice() {
        [] => (String::new(), String::new()),
        [one] if one.contains(':') => (String::new(), one.to_string()),
        rest => {
            let eta = rest.last().filter(|s| s.contains(':')).map(|s| s.to_string()).unwrap_or_default();
            let n = if eta.is_empty() { rest.len() } else { rest.len() - 1 };
            (rest[..n].join(" "), eta)
        }
    };
    Some((item, head, pct, speed, eta))
}

pub fn render(root: &Path, text: &str) -> String {
    let lines: Vec<&str> = text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect();
    let done: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with("Скачано:")).collect();
    let errors: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with("Ошибка")).collect();
    let total: usize = lines
        .iter()
        .rev()
        .find_map(|l| {
            let rest = l.strip_prefix("Плейлист «")?;
            let after = rest.rsplit_once("»: ")?.1;
            after.split(' ').next()?.parse().ok()
        })
        .unwrap_or(0);
    let progress = lines.iter().rev().find(|l| parse_progress(l).is_some()).copied().unwrap_or("");
    // Архив переживает перезапуски, журнал — нет.
    let archived = std::fs::read_to_string(root.join(".ytdl_archive.txt"))
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(done.len());

    let mut out = vec![format!("Папка: {}", root.display()), String::new()];
    if let Some((item, what, pct, speed, eta)) = parse_progress(progress) {
        let filled = ((pct / 2.5) as usize).min(40);
        let bar = "█".repeat(filled) + &"░".repeat(40 - filled);
        out.push(format!("Сейчас: {what}{}", if item.is_empty() { String::new() } else { format!("   (видео {item})") }));
        out.push(format!("[{bar}] {pct:.1}%   {speed}   осталось {}", if eta.is_empty() { "—".into() } else { eta }));
    }
    out.push(String::new());
    let mut ready = format!("Готово видео: {archived}");
    if total > 0 {
        ready += &format!(" из {total}");
    }
    if archived != done.len() {
        ready += &format!("   (в этом запуске: {})", done.len());
    }
    out.push(ready);
    out.push(format!("Ошибок: {}", errors.len()));
    if let Some(free) = free_gb(&root.to_string_lossy()) {
        out.push(format!("Свободно на диске: {free:.0} ГБ"));
    }
    out.push(String::new());
    if !done.is_empty() {
        out.push("Последние:".into());
        for l in done.iter().rev().take(5).rev() {
            let p = l.trim_start_matches("Скачано:").trim().split("  [").next().unwrap_or("");
            let shown = PathBuf::from(p)
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| p.to_string());
            out.push(format!("  {shown}"));
        }
    }
    if let Some(e) = errors.last() {
        out.push(String::new());
        out.push("Последняя ошибка:".into());
        out.push(format!("  {}", e.chars().take(200).collect::<String>()));
    }
    let tail_start = lines.len().saturating_sub(3);
    let finished = lines[tail_start..].iter().any(|l| l.starts_with("Готово:") || l.starts_with("Новых видео нет"));
    out.push(String::new());
    out.push(if finished {
        "Загрузка завершена.".into()
    } else {
        "Обновляется каждые 2 с · Ctrl+C — закрыть просмотр (загрузка продолжится)".into()
    });
    out.join("\n")
}

#[cfg(windows)]
fn enable_ansi() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> isize;
        fn GetConsoleMode(h: isize, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: isize, mode: u32) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(h, &mut mode) != 0 {
            SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[cfg(not(windows))]
fn enable_ansi() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_line() {
        let (item, what, pct, speed, eta) =
            parse_progress("3/25 Видео 1080p · 120.0 МБ                     42.5%    3.1 МБ/с     0:31").unwrap();
        assert_eq!(item, "3/25");
        assert_eq!(what, "Видео 1080p · 120.0 МБ");
        assert!((pct - 42.5).abs() < 1e-9);
        assert_eq!(speed, "3.1 МБ/с");
        assert_eq!(eta, "0:31");
    }
}
