//! Инструкция «КАК СДЕЛАТЬ ТЕМУ.html» с картинками — встроена в программу и кладётся в папку тем
//! (при запуске окна и по кнопке «папка тем»). Обновляется, если версия в программе новее;
//! файлы тем пользователя не трогаются — пишутся только файлы инструкции.
//!
//! Исходники — `rust/assets/theme-guide/`, список файлов — `guide_files.rs` (собирается скриптом вместе со скриншотами).

use std::path::Path;

/// Версия инструкции: увеличить, когда меняются её файлы.
pub const VERSION: &str = "1";
/// Папка с картинками внутри папки тем.
pub const DIR: &str = "инструкция";
pub const HTML: &str = "КАК СДЕЛАТЬ ТЕМУ.html";
const MARKER: &str = "версия.txt";

include!("guide_files.rs");

/// Положить инструкцию в папку тем, если её нет или она старее. true — файлы записаны.
pub fn ensure(dir: &Path) -> std::io::Result<bool> {
    let marker = dir.join(DIR).join(MARKER);
    if std::fs::read_to_string(&marker).map(|v| v.trim() == VERSION).unwrap_or(false) && dir.join(HTML).is_file() {
        return Ok(false);
    }
    for (rel, bytes) in FILES {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(p, bytes)?;
    }
    std::fs::write(marker, VERSION)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_guide_once_and_keeps_user_files() {
        let dir = std::env::temp_dir().join(format!("vgd-guide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("моя.json"), "{}").unwrap();
        assert!(ensure(&dir).unwrap());
        assert!(dir.join(HTML).is_file());
        assert!(FILES.iter().all(|(rel, _)| dir.join(rel).is_file()));
        assert!(!ensure(&dir).unwrap(), "второй раз — ничего не пишется");
        // версия старее — обновляется, тема пользователя на месте
        std::fs::write(dir.join(DIR).join(MARKER), "0").unwrap();
        assert!(ensure(&dir).unwrap());
        assert_eq!(std::fs::read_to_string(dir.join("моя.json")).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_links_only_existing_images() {
        let html = FILES.iter().find(|(r, _)| *r == HTML).map(|(_, b)| String::from_utf8_lossy(b).into_owned()).expect("html");
        for part in html.split("src=\"").skip(1) {
            let src = &part[..part.find('"').unwrap()];
            assert!(FILES.iter().any(|(r, _)| r.replace('\\', "/") == src), "нет картинки {src}");
        }
    }
}
