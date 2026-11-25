//! Поиск музыки на публичных торрент-трекерах. Трекер (apibay/PirateBay) ищет
//! по названию раздачи и отдаёт info_hash — magnet строим сами, файл качает
//! [`super::torrent::TorrentProvider`] выборочно, только FLAC.
//!
//! Текстовый запрос — это имя раздачи «Artist - Album» либо «Artist - Track».
//! Трекер ищёт по вхождению слов, поэтому отдаёт «раздачи, где-то внутри
//! которых» есть подходящие файлы; финальный выбор — по именам файлов
//! метаданных (эвристика в [`pick_track_file`]).

use std::time::Duration;

use serde::Deserialize;
use tracing::debug;

use super::{P2pError, SearchQuery, SourceLocator};

/// Каталог «Music» у apibay (PirateBay).
const TPB_CATEGORY_MUSIC: u64 = 100;

/// Ответ apibay `/q.php` (поля как у TPB).
#[derive(Debug, Deserialize)]
struct ApibayHit {
    name: String,
    info_hash: String,
    seeders: String,
    #[allow(dead_code)]
    leechers: String,
    size: String,
    #[allow(dead_code)]
    num_files: String,
}

/// Раздача с magnet и именем.
#[derive(Clone, Debug)]
pub struct TrackerHit {
    pub name: String,
    pub magnet: String,
    pub seeders: i64,
    pub size: u64,
}

impl TrackerHit {
    /// Кандидат для докачки: magnet + нулевой файл. Реальный выбор файла
    /// происходит после получения метаданных — по [`pick_track_file`].
    pub fn to_locator(&self) -> SourceLocator {
        SourceLocator::Torrent { magnet: self.magnet.clone(), file_index: 0 }
    }
}

/// Поиск FLAC по запросу «artist title» на apibay (зеркала PirateBay).
/// Offline-тесты не ходят в сеть; сетевые — под флагом `live` в тестах.
///
/// Любой сбой apibay (провайдер рвёт TCP RST, таймаут, битый JSON) — это «пусто»,
/// а не ошибка сети: P2P-каскад просто идёт к следующему источнику, а очередь
/// загрузки не решает, что у пользователя пропал интернет.
pub fn search_trackers(query: &str, timeout: Duration) -> Result<Vec<TrackerHit>, P2pError> {
    let url = format!(
        "https://apibay.org/q.php?q={}&cat={}",
        encode_query_component(query),
        TPB_CATEGORY_MUSIC
    );
    // 3 секунды на соединение: заблокированный хост рвётся RST или молчит —
    // и то и другое отпускает поиск почти сразу, не дожидаясь общего таймаута.
    let response = crate::http::get(&url)
        .connect_timeout(3)
        .timeout(timeout.as_secs())
        .text();
    let text = match response {
        Ok(t) => t,
        Err(e) => {
            debug!(error = %e, "apibay unavailable, continuing without trackers");
            return Ok(vec![]);
        }
    };
    let hits: Vec<ApibayHit> = match serde_json::from_str(&text) {
        Ok(h) => h,
        Err(e) => {
            debug!(error = %e, "apibay response is not JSON, continuing without trackers");
            return Ok(vec![]);
        }
    };
    let mut out: Vec<TrackerHit> = hits
        .into_iter()
        .filter(|h| h.seeders.parse::<i64>().unwrap_or(0) > 0 && h.info_hash.len() >= 40)
        .map(|h| TrackerHit {
            magnet: format!("magnet:?xt=urn:btih:{}", &h.info_hash[..40]),
            seeders: h.seeders.parse().unwrap_or(0),
            size: h.size.parse().unwrap_or(0),
            name: h.name,
        })
        .collect();
    // Живее сиды — выше в списке.
    out.sort_by(|a, b| b.seeders.cmp(&a.seeders));
    Ok(out)
}

/// URL-кодирование компонента запроса (пробел → `%20`, кириллица → UTF-8 %).
pub(crate) fn encode_query_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Выбрать файл трека из списка файлов раздачи по названию запроса.
/// FLAC только: без потерь P2P не имеет смысла, сервисы дают сжатое сами.
pub fn pick_track_file(query: &SearchQuery, files: &[super::TorrentFile]) -> Option<usize> {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>();
    let artist = norm(&query.artist);
    let title = norm(&query.title);
    let mut best: Option<(usize, usize, u64)> = None; // (индекс, score, размер)
    for f in files {
        let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = norm(&name);
        let is_flac = f.path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("flac"));
        if !is_flac || f.size == 0 {
            continue;
        }
        let artist_ok = !artist.is_empty() && name.contains(&artist);
        let title_ok = !title.is_empty() && name.contains(&title);
        // Название трека важнее артиста: раздача уже найдена по нему.
        let score = title_ok as usize * 2 + artist_ok as usize;
        if score == 0 {
            continue;
        }
        // Не обложка и не лог/плейлист.
        if name.contains("cover") || name.contains("scan") || name.contains("cue") {
            continue;
        }
        let smaller_is_better = f.size;
        if best.is_none()
            || score > best.unwrap().1
            || (score == best.unwrap().1 && smaller_is_better < best.unwrap().2)
        {
            best = Some((f.index, score, f.size));
        }
    }
    best.map(|(index, _, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(index: usize, name: &str, size: u64) -> super::super::TorrentFile {
        super::super::TorrentFile {
            index,
            path: PathBuf::from(name),
            size,
            absolute_path: PathBuf::from(name),
        }
    }

    #[test]
    fn encodes_query_component() {
        assert_eq!(encode_query_component("daft punk one more time"), "daft%20punk%20one%20more%20time");
        assert_eq!(encode_query_component("ä ö"), "%C3%A4%20%C3%B6");
    }

    #[test]
    fn picks_flac_matching_title() {
        let q = SearchQuery { artist: "Daft Punk".into(), title: "One More Time".into(), album: None, duration: None, limit: 4 };
        let files = vec![
            file(0, "cover.jpg", 100),
            file(1, "01 - One More Time.flac", 40_000_000),
            file(2, "02 - Aerodynamic.flac", 30_000_000),
        ];
        assert_eq!(pick_track_file(&q, &files), Some(1));
    }

    #[test]
    fn prefers_title_plus_artist_over_title_only() {
        let q = SearchQuery { artist: "Daft Punk".into(), title: "Face To Face".into(), album: None, duration: None, limit: 4 };
        let files = vec![
            file(0, "Face To Face.flac", 40_000_000),
            file(1, "Daft Punk - Face To Face.flac", 41_000_000),
        ];
        assert_eq!(pick_track_file(&q, &files), Some(1));
    }

    #[test]
    fn skips_covers_and_cues() {
        let q = SearchQuery { artist: "X".into(), title: "X".into(), album: None, duration: None, limit: 4 };
        let files = vec![
            file(0, "cover X.flac", 100),
            file(1, "cue X.flac", 100),
            file(2, "X.flac", 100),
        ];
        assert_eq!(pick_track_file(&q, &files), Some(2));
    }

    #[test]
    fn magnet_from_apibay_hit() {
        let hit = TrackerHit {
            name: "Daft Punk - Discovery".into(),
            magnet: "magnet:?xt=urn:btih:495A6CA4060815F2E8C7A5FEFFE4557FA4962787".into(),
            seeders: 12,
            size: 300_000_000,
        };
        assert!(hit.magnet.starts_with("magnet:?xt=urn:btih:"));
        assert!(matches!(hit.to_locator(), super::super::SourceLocator::Torrent { .. }));
    }

    #[tokio::test]
    #[ignore = "живой запрос к apibay"]
    async fn live_apibay_daft_punk() {
        let hits = search_trackers("daft punk one more time", Duration::from_secs(20)).expect("apibay доступен");
        assert!(!hits.is_empty(), "apibay должен найти хотя бы одну раздачу");
        assert!(hits[0].seeders > 0);
        assert!(hits[0].magnet.starts_with("magnet:?xt=urn:btih:"));
    }
}
