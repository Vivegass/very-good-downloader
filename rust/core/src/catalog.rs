//! Каталог для выбора по названию: исполнители, альбомы и треки (Deezer — открытый API без ключа),
//! и релизы исполнителя для окна дискографии. Общее для ПК и Android.
//!
//! Выбранное превращается в обычную ссылку Deezer, которую уже понимает `music::resolve`:
//! трек — `deezer.com/track/ID`, альбом — `deezer.com/album/ID`, выбранные релизы —
//! `deezer.com/artist/ID#vgd-pick=1,2,3`, популярное — `deezer.com/artist/ID#vgd-top`.
//! Звук, как всегда, ищется в аккаунтах пользователя и других источниках.

use serde_json::Value;

use crate::music::{get_json, qenc, s};

const API: &str = "https://api.deezer.com";

/// Найденное: исполнитель, альбом или трек.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hit {
    /// "artist" / "album" / "track"
    pub kind: String,
    pub id: i64,
    pub title: String,
    /// вторая строка: «12 альбомов» / «Daft Punk · 2001 · 14 треков» / «Daft Punk · Discovery · 5:20»
    pub sub: String,
    /// маленькая обложка или фото (до 250×250)
    pub cover: String,
    /// ссылка, которую ставить в очередь (у исполнителя — его страница, для окна дискографии)
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub artists: Vec<Hit>,
    pub albums: Vec<Hit>,
    pub tracks: Vec<Hit>,
}

impl Catalog {
    pub fn is_empty(&self) -> bool {
        self.artists.is_empty() && self.albums.is_empty() && self.tracks.is_empty()
    }
}

fn plural(n: u64, one: &str, few: &str, many: &str) -> String {
    let (m10, m100) = (n % 10, n % 100);
    let w = if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    };
    format!("{n} {w}")
}

fn mmss(sec: u64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

fn data(v: &Value) -> Vec<Value> {
    v["data"].as_array().cloned().unwrap_or_default()
}

/// Поиск по названию: до 3 исполнителей, 6 альбомов и 8 треков (три запроса одновременно).
pub fn search(q: &str) -> Result<Catalog, String> {
    let q = q.trim();
    if q.chars().count() < 2 {
        return Ok(Catalog::default());
    }
    let enc = qenc(q);
    let (artists, albums, tracks) = std::thread::scope(|sc| {
        let a = sc.spawn(|| get_json(&format!("{API}/search/artist?q={enc}&limit=3"), &[]));
        let b = sc.spawn(|| get_json(&format!("{API}/search/album?q={enc}&limit=6"), &[]));
        let c = sc.spawn(|| get_json(&format!("{API}/search?q={enc}&limit=8"), &[]));
        (a.join(), b.join(), c.join())
    });
    let (artists, albums, tracks) = (
        artists.map_err(|_| "поиск сломался".to_string())?,
        albums.map_err(|_| "поиск сломался".to_string())?,
        tracks.map_err(|_| "поиск сломался".to_string())?,
    );
    // Хоть один ответ — показываем; все три с ошибкой — ошибка (обычно нет сети).
    if let (Err(e), Err(_), Err(_)) = (&artists, &albums, &tracks) {
        return Err(e.clone());
    }
    let mut out = Catalog::default();
    if let Ok(v) = artists {
        out.artists = data(&v)
            .iter()
            .map(|a| {
                let n = a["nb_album"].as_u64().unwrap_or(0);
                Hit {
                    kind: "artist".into(),
                    id: a["id"].as_i64().unwrap_or(0),
                    title: s(a, "name"),
                    sub: if n > 0 { format!("исполнитель · {}", plural(n, "релиз", "релиза", "релизов")) } else { "исполнитель".into() },
                    cover: s(a, "picture_medium"),
                    url: format!("https://www.deezer.com/artist/{}", a["id"]),
                }
            })
            .filter(|h| h.id > 0 && !h.title.is_empty())
            .collect();
    }
    if let Ok(v) = albums {
        out.albums = data(&v)
            .iter()
            .map(|a| {
                let n = a["nb_tracks"].as_u64().unwrap_or(0);
                let kind = match s(a, "record_type").as_str() {
                    "single" => "сингл",
                    "ep" => "EP",
                    "compile" => "сборник",
                    _ => "альбом",
                };
                let mut sub = vec![s(&a["artist"], "name"), kind.to_string()];
                if n > 0 {
                    sub.push(plural(n, "трек", "трека", "треков"));
                }
                Hit {
                    kind: "album".into(),
                    id: a["id"].as_i64().unwrap_or(0),
                    title: s(a, "title"),
                    sub: sub.into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
                    cover: s(a, "cover_medium"),
                    url: format!("https://www.deezer.com/album/{}", a["id"]),
                }
            })
            .filter(|h| h.id > 0 && !h.title.is_empty())
            .collect();
    }
    if let Ok(v) = tracks {
        out.tracks = data(&v)
            .iter()
            .map(|t| {
                let mut sub = vec![s(&t["artist"], "name"), s(&t["album"], "title")];
                if let Some(d) = t["duration"].as_u64().filter(|d| *d > 0) {
                    sub.push(mmss(d));
                }
                Hit {
                    kind: "track".into(),
                    id: t["id"].as_i64().unwrap_or(0),
                    title: s(t, "title"),
                    sub: sub.into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
                    cover: s(&t["album"], "cover_medium"),
                    url: format!("https://www.deezer.com/track/{}", t["id"]),
                }
            })
            .filter(|h| h.id > 0 && !h.title.is_empty())
            .collect();
    }
    Ok(out)
}

/// Релиз исполнителя.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Release {
    pub id: i64,
    pub title: String,
    /// "album" / "ep" / "single" / "compile"
    pub kind: String,
    /// «2001»
    pub year: String,
    pub cover: String,
    /// концертный (Live, Alive 2007, Unplugged)
    pub live: bool,
    /// другая версия уже есть (Deluxe, Remaster, 10th Anniversary, Clean…) — «без повторов» её снимает
    pub dup: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub picture: String,
    /// альбомы, EP, синглы, сборники — в этом порядке, внутри — новые первыми
    pub releases: Vec<Release>,
}

/// Название без пометок версии: «Homework (25th Anniversary Edition)» → «homework».
fn base_title(t: &str) -> String {
    const MARKS: [&str; 14] = [
        "edition", "deluxe", "remaster", "anniversary", "expanded", "clean", "explicit", "version", "drumless", "bonus",
        "edit", "feat", "radio", "super",
    ];
    let mut out = String::new();
    let mut rest = t;
    // Скобки ( … ) и [ … ] с пометкой версии убираем, остальные оставляем.
    while let Some(i) = rest.find(['(', '[']) {
        out.push_str(&rest[..i]);
        let close = if rest[i..].starts_with('(') { ')' } else { ']' };
        let end = rest[i..].find(close).map(|e| i + e + 1).unwrap_or(rest.len());
        let inner = rest[i..end].to_lowercase();
        if !MARKS.iter().any(|m| inner.contains(m)) {
            out.push_str(&rest[i..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    // «Album - Remastered 2011»
    let low = out.to_lowercase();
    let cut = low.find(" - ").filter(|i| MARKS.iter().any(|m| low[*i..].contains(m))).unwrap_or(low.len());
    simple(&low[..cut])
}

/// Нижний регистр, без лишних пробелов и знаков по краям (скобки не трогаем — в отличие от music::normalize).
fn simple(t: &str) -> String {
    t.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(|c: char| !c.is_alphanumeric() && c != ')' && c != ']').to_string()
}

fn is_live(t: &str) -> bool {
    let low = t.to_lowercase();
    low.split(|c: char| !c.is_alphanumeric()).any(|w| w == "live" || w == "unplugged")
        || low.starts_with("alive ")
        || low.contains("(live")
}

/// Релизы исполнителя (Deezer): список без треков — быстро, для окна дискографии.
pub fn artist(id: i64) -> Result<Artist, String> {
    let a = get_json(&format!("{API}/artist/{id}"), &[])?;
    if a.get("error").is_some() {
        return Err("исполнитель не найден в каталоге".into());
    }
    let mut list = Vec::new();
    let mut next = Some(format!("{API}/artist/{id}/albums?limit=100"));
    let mut pages = 0;
    while let (Some(u), true) = (next, pages < 20) {
        let page = get_json(&u, &[])?;
        list.extend(data(&page));
        next = page["next"].as_str().map(String::from);
        pages += 1;
    }
    let rank = |k: &str| match k {
        "album" => 0,
        "ep" => 1,
        "single" => 2,
        _ => 3,
    };
    let mut releases: Vec<(String, Release)> = list
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_i64()?;
            let title = s(r, "title");
            let kind = match s(r, "record_type").as_str() {
                k @ ("album" | "ep" | "single" | "compile") => k.to_string(),
                _ => "album".to_string(),
            };
            let date = s(r, "release_date");
            Some((
                date.clone(),
                Release { id, live: is_live(&title), title, kind, year: date.chars().take(4).collect(), cover: s(r, "cover_medium"), dup: false },
            ))
        })
        .collect();
    releases.sort_by(|(da, a), (db, b)| rank(&a.kind).cmp(&rank(&b.kind)).then(db.cmp(da)));
    // Повторы: одно и то же базовое название. Остаётся версия без пометок, иначе самая ранняя.
    let mut groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, (_, r)) in releases.iter().enumerate() {
        groups.entry(base_title(&r.title)).or_default().push(i);
    }
    for idx in groups.values().filter(|v| v.len() > 1) {
        let keep = idx
            .iter()
            .copied()
            .find(|&i| simple(&releases[i].1.title) == base_title(&releases[i].1.title))
            .or_else(|| idx.iter().copied().min_by(|&a, &b| releases[a].0.cmp(&releases[b].0)))
            .unwrap_or(idx[0]);
        for &i in idx {
            if i != keep {
                releases[i].1.dup = true;
            }
        }
    }
    Ok(Artist { id, name: s(&a, "name"), picture: s(&a, "picture_medium"), releases: releases.into_iter().map(|(_, r)| r).collect() })
}

/// Ссылка «скачать выбранные релизы» (одна задача в очереди, папки «год - альбом»).
pub fn pick_url(artist_id: i64, ids: &[i64]) -> String {
    let ids: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
    format!("https://www.deezer.com/artist/{artist_id}#vgd-pick={}", ids.join(","))
}

/// Ссылка «популярные треки исполнителя».
pub fn top_url(artist_id: i64) -> String {
    format!("https://www.deezer.com/artist/{artist_id}#vgd-top")
}

/// Что выбрано в окне по умолчанию: альбомы и EP, без концертных и повторов.
pub fn default_pick(r: &Release) -> bool {
    matches!(r.kind.as_str(), "album" | "ep") && !r.live && !r.dup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(base_title("Homework (25th Anniversary Edition)"), base_title("Homework"));
        assert_eq!(base_title("Random Access Memories (Drumless Edition)"), base_title("Random Access Memories"));
        assert_eq!(base_title("Abbey Road - Remastered 2019"), base_title("Abbey Road"));
        assert_ne!(base_title("Human After All (Remixes)"), base_title("Human After All"));
        assert!(is_live("Alive 2007"));
        assert!(is_live("MTV Unplugged in New York"));
        assert!(is_live("Live at Wembley"));
        assert!(!is_live("Oliver"));
        assert!(!is_live("Deliver"));
    }

    #[test]
    fn words() {
        assert_eq!(plural(1, "трек", "трека", "треков"), "1 трек");
        assert_eq!(plural(3, "трек", "трека", "треков"), "3 трека");
        assert_eq!(plural(11, "трек", "трека", "треков"), "11 треков");
        assert_eq!(plural(21, "трек", "трека", "треков"), "21 трек");
        assert_eq!(pick_url(27, &[1, 2]), "https://www.deezer.com/artist/27#vgd-pick=1,2");
    }

    #[test]
    #[ignore] // сеть: cargo test -p vgd-core catalog -- --ignored --nocapture
    fn live() {
        let c = search("daft punk").unwrap();
        println!("{:?}", c.artists.iter().map(|a| (&a.title, &a.sub)).collect::<Vec<_>>());
        assert!(c.artists.iter().any(|a| a.title == "Daft Punk"));
        let c = search("daft punk discovery").unwrap();
        println!("{:?}", c.albums.iter().map(|a| (&a.title, &a.sub)).collect::<Vec<_>>());
        println!("{:?}", c.tracks.iter().map(|a| (&a.title, &a.sub)).collect::<Vec<_>>());
        assert!(c.albums.iter().any(|a| a.title.contains("Discovery")));
        let a = artist(27).unwrap();
        for r in &a.releases {
            println!("{} {} {} live={} dup={} pick={}", r.kind, r.year, r.title, r.live, r.dup, default_pick(r));
        }
        assert!(a.releases.iter().any(|r| r.title == "Discovery" && default_pick(r)));
        assert!(a.releases.iter().any(|r| r.title.starts_with("Homework (25th") && r.dup));
        let l = crate::music::resolve(&pick_url(27, &[a.releases.iter().find(|r| r.title == "Discovery").unwrap().id]), &crate::settings::Settings::default(), &|m| println!("{m}"))
            .unwrap()
            .unwrap();
        assert_eq!(l.tracks.len(), 14);
    }
}

