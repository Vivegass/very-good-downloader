//! Что открывать у готовой загрузки: значок «показать в папке» и щелчок по названию карточки.
//!
//! - одно видео (и, может быть, копия для монтажа `…_edit.mp4/.mov`, субтитры, обложка) — главный файл:
//!   первый медиафайл, не копия для монтажа; если остался только `_edit` — он;
//! - несколько треков/видео (плейлист, альбом) — их общая папка;
//! - файл удалили или перенесли — его папка; нет и папки — сообщение для журнала.

use std::path::{Path, PathBuf};

/// Куда вести пользователя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// файл есть: «показать в папке» — выделить его в Проводнике, щелчок — открыть программой по умолчанию
    File(PathBuf),
    /// плейлист/альбом или файла уже нет — открыть папку
    Folder(PathBuf),
    /// нет ни файла, ни папки — текст для журнала
    Missing(String),
}

/// Сопутствующие файлы — не «главный» результат загрузки.
fn is_sidecar(p: &Path) -> bool {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "srt" | "vtt" | "ass" | "ssa" | "lrc" | "jpg" | "jpeg" | "png" | "webp" | "txt" | "json" | "part" | "ytdl" | "nfo"
    )
}

/// Копия для монтажа: `<имя>_edit.mp4` / `<имя>_edit.mov`.
fn is_edit_copy(p: &Path) -> bool {
    p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with("_edit"))
}

/// Медиафайлы загрузки по порядку: сначала оригиналы, копии для монтажа — после.
fn media(files: &[String]) -> Vec<PathBuf> {
    let all: Vec<PathBuf> = files.iter().map(|f| PathBuf::from(f.trim())).filter(|p| !p.as_os_str().is_empty()).collect();
    let mut main: Vec<PathBuf> = all.iter().filter(|p| !is_sidecar(p) && !is_edit_copy(p)).cloned().collect();
    main.extend(all.iter().filter(|p| !is_sidecar(p) && is_edit_copy(p)).cloned());
    if main.is_empty() {
        main = all; // одни субтитры/обложки — пусть хоть они
    }
    main
}

/// Самая глубокая общая папка у файлов.
fn common_dir(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut dir = paths.first()?.parent()?.to_path_buf();
    for p in &paths[1..] {
        while !p.starts_with(&dir) {
            dir = dir.parent()?.to_path_buf();
        }
    }
    (!dir.as_os_str().is_empty()).then_some(dir)
}

/// Несколько треков/видео (плейлист, альбом) — открывается папка. Без обращения к диску (для подсказок).
pub fn is_collection(files: &[String]) -> bool {
    media(files).iter().filter(|p| !is_edit_copy(p)).count() > 1
}

/// Главный файл загрузки: первый существующий оригинал (не копия для монтажа), иначе первый по списку.
pub fn main_file(files: &[String]) -> Option<PathBuf> {
    let m = media(files);
    m.iter().find(|p| p.is_file()).cloned().or_else(|| m.into_iter().next())
}

/// Что показать/открыть для готовой загрузки с файлами `files`.
pub fn target(files: &[String]) -> Target {
    let m = media(files);
    if m.is_empty() {
        return Target::Missing("у загрузки нет файлов".into());
    }
    // Плейлист, альбом, канал: несколько файлов — их папка.
    let originals: Vec<PathBuf> = m.iter().filter(|p| !is_edit_copy(p)).cloned().collect();
    if originals.len() > 1 {
        if let Some(d) = common_dir(&originals) {
            if d.is_dir() {
                return Target::Folder(d);
            }
        }
    }
    if let Some(f) = m.iter().find(|p| p.is_file()) {
        return Target::File(f.clone());
    }
    // Файла нет (удалили, перенесли) — ближайшая существующая папка, где он был.
    for p in &m {
        if let Some(d) = p.parent().filter(|d| d.is_dir()) {
            return Target::Folder(d.to_path_buf());
        }
    }
    let shown = m[0].parent().unwrap_or(&m[0]).display().to_string();
    Target::Missing(format!("файла и папки больше нет: {shown}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vgd-opener-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn s(p: &Path) -> String {
        p.display().to_string()
    }

    #[test]
    fn main_file_skips_edit_copy_and_sidecars() {
        let d = tmp("main");
        let video = d.join("Канал - Видео.mkv");
        let edit = d.join("Канал - Видео_edit.mp4");
        let subs = d.join("Канал - Видео.ru.srt");
        for f in [&video, &edit, &subs] {
            std::fs::write(f, b"x").unwrap();
        }
        let files = vec![s(&subs), s(&edit), s(&video)];
        assert_eq!(main_file(&files).unwrap(), video);
        assert_eq!(target(&files), Target::File(video.clone()));
        // «копия вместо оригинала»: остался только _edit
        assert_eq!(target(&[s(&edit), s(&subs)]), Target::File(edit.clone()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn deleted_file_falls_back_to_folder_then_message() {
        let d = tmp("gone");
        let gone = d.join("удалён.mp4");
        assert_eq!(target(&[s(&gone)]), Target::Folder(d.clone()));
        // первый удалён, второй (копия для монтажа) на месте — показать её
        let edit = d.join("удалён_edit.mov");
        std::fs::write(&edit, b"x").unwrap();
        assert_eq!(target(&[s(&gone), s(&edit)]), Target::File(edit));
        let _ = std::fs::remove_dir_all(&d);
        match target(&[s(&gone)]) {
            Target::Missing(m) => assert!(m.contains("больше нет"), "{m}"),
            t => panic!("{t:?}"),
        }
        assert!(matches!(target(&[]), Target::Missing(_)));
    }

    #[test]
    fn playlist_opens_common_folder() {
        let d = tmp("pl");
        let pl = d.join("Плейлист");
        std::fs::create_dir_all(pl.join("CD2")).unwrap();
        let a = pl.join("01 - Трек.mp3");
        let b = pl.join("CD2").join("02 - Трек.mp3");
        let lrc = pl.join("01 - Трек.lrc");
        for f in [&a, &b, &lrc] {
            std::fs::write(f, b"x").unwrap();
        }
        assert_eq!(target(&[s(&a), s(&lrc), s(&b)]), Target::Folder(pl.clone()));
        // один трек — сам файл
        assert_eq!(target(&[s(&a), s(&lrc)]), Target::File(a.clone()));
        // видео + копия для монтажа — не плейлист
        let v = pl.join("v.mkv");
        let ve = pl.join("v_edit.mp4");
        std::fs::write(&v, b"x").unwrap();
        std::fs::write(&ve, b"x").unwrap();
        assert_eq!(target(&[s(&v), s(&ve)]), Target::File(v));
        let _ = std::fs::remove_dir_all(&d);
    }
}
