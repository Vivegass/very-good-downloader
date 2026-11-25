//! Аккаунты: откуда брать звук и с каким качеством, если пользователь вошёл в сервис.
//!
//! * VK и Яндекс Музыка — свои входы (`vk.rs`, `yandex_acc.rs`).
//! * YouTube / YouTube Music и SoundCloud — куки вошедшего браузера в файле `accounts/<сервис>.txt`
//!   (формат Netscape, его читает yt-dlp). На ПК куки присылает расширение браузера (только на
//!   127.0.0.1 — в программу), на телефоне — окно входа. С Premium YouTube Music отдаёт AAC 256 kbps
//!   (формат 141) вместо Opus ~160, с Go+ SoundCloud — AAC 256 вместо 160.
//! * Сервисы, где пользователь вошёл, ищутся первыми (`source_order`).
//!
//! Звук Spotify, Tidal, Deezer, Qobuz, Звука и Splice программа не качает и с аккаунтом:
//! он защищён (DRM / шифрование). Вход там нужен только для списков треков (через расширение).

use serde_json::Value;
use std::path::PathBuf;

use crate::appinfo::data_dir;
use crate::settings::Settings;

/// Сервис, вход в который — куки браузера: ключ, название, домены кук, имена кук, по которым видно вход.
pub struct CookieService {
    pub key: &'static str,
    pub name: &'static str,
    pub domains: &'static [&'static str],
    pub auth: &'static [&'static str],
}

pub const COOKIE_SERVICES: [CookieService; 2] = [
    CookieService {
        key: "youtube",
        name: "YouTube",
        domains: &["youtube.com"],
        auth: &["SAPISID", "__Secure-3PAPISID", "__Secure-1PAPISID"],
    },
    CookieService { key: "soundcloud", name: "SoundCloud", domains: &["soundcloud.com"], auth: &["oauth_token"] },
];

fn service(key: &str) -> Option<&'static CookieService> {
    COOKIE_SERVICES.iter().find(|s| s.key == key)
}

/// Файл кук сервиса.
pub fn file(key: &str) -> PathBuf {
    data_dir().join("accounts").join(format!("{key}.txt"))
}

/// Вошёл ли пользователь: в файле есть кука входа.
pub fn logged_in(key: &str) -> bool {
    let Some(svc) = service(key) else { return false };
    let Ok(text) = std::fs::read_to_string(file(key)) else { return false };
    text.lines().filter(|l| !l.starts_with("# ")).any(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        f.len() >= 7 && svc.auth.contains(&f[5]) && !f[6].is_empty()
    })
}

/// Файл кук, если вход есть.
pub fn cookies_file(key: &str) -> Option<PathBuf> {
    logged_in(key).then(|| file(key))
}

pub fn logout(key: &str) {
    let _ = std::fs::remove_file(file(key));
}

fn domain_ok(svc: &CookieService, domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_ascii_lowercase();
    svc.domains.iter().any(|s| d == *s || d.ends_with(&format!(".{s}")))
}

/// Куки от расширения (объекты `chrome.cookies`: domain, path, secure, expirationDate, name, value,
/// hostOnly) → файл сервиса. Чужие домены отбрасываются. Ok(сколько кук записано, есть ли вход).
pub fn save_json(key: &str, cookies: &Value) -> Result<(usize, bool), String> {
    let svc = service(key).ok_or_else(|| format!("неизвестный сервис «{key}»"))?;
    let list = cookies.as_array().ok_or("нет кук")?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut lines = vec![
        "# Netscape HTTP Cookie File".to_string(),
        format!("# Very Good Downloader: вход в {} (из браузера)", svc.name),
    ];
    for c in list {
        let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (domain, name, value) = (s("domain"), s("name"), s("value"));
        if name.is_empty() || !domain_ok(svc, &domain) || [&domain, &name, &value].iter().any(|v| v.contains(['\t', '\n', '\r'])) {
            continue;
        }
        let host_only = c.get("hostOnly").and_then(Value::as_bool).unwrap_or(false);
        let domain = if host_only { domain.trim_start_matches('.').to_string() } else { format!(".{}", domain.trim_start_matches('.')) };
        let path = Some(s("path")).filter(|p| p.starts_with('/')).unwrap_or_else(|| "/".into());
        let secure = c.get("secure").and_then(Value::as_bool).unwrap_or(true);
        // Куки сеанса (без срока) yt-dlp может отбросить — даём им месяц.
        let exp = c.get("expirationDate").and_then(Value::as_f64).map(|e| e as u64).filter(|e| *e > 0).unwrap_or(now + 30 * 24 * 3600);
        let sub = if host_only { "FALSE" } else { "TRUE" };
        let sec = if secure { "TRUE" } else { "FALSE" };
        lines.push(format!("{domain}\t{sub}\t{path}\t{sec}\t{exp}\t{name}\t{value}"));
    }
    let n = lines.len() - 2;
    if n == 0 {
        return Err("куки не пришли".into());
    }
    write(key, &(lines.join("\n") + "\n"))?;
    Ok((n, logged_in(key)))
}

/// Готовый файл Netscape (окно входа на телефоне, cookies.txt) → файл сервиса (только его домены).
pub fn save_netscape(key: &str, text: &str) -> Result<(usize, bool), String> {
    let svc = service(key).ok_or_else(|| format!("неизвестный сервис «{key}»"))?;
    let mut lines = vec!["# Netscape HTTP Cookie File".to_string(), format!("# Very Good Downloader: вход в {}", svc.name)];
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let domain = f.first().map(|d| d.trim_start_matches("#HttpOnly_")).unwrap_or("");
        if f.len() >= 7 && !l.starts_with("# ") && domain_ok(svc, domain) {
            lines.push(l.trim_end_matches('\r').to_string());
        }
    }
    let n = lines.len() - 2;
    if n == 0 {
        return Err(format!("нет кук {}", svc.domains.join(", ")));
    }
    write(key, &(lines.join("\n") + "\n"))?;
    Ok((n, logged_in(key)))
}

fn write(key: &str, text: &str) -> Result<(), String> {
    let f = file(key);
    if let Some(p) = f.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, text).map_err(|e| format!("не записать {}: {e}", f.display()))
}

/// Включён ли сервис в настройках (галочка «брать звук от аккаунта»).
pub fn wanted(s: &Settings, key: &str) -> bool {
    match key {
        "youtube" => s.ytm_acc,
        "soundcloud" => s.sc_acc,
        _ => false,
    }
}

// ---------- откуда брать звук ----------

/// Источники звука для поиска замены: ключ и название (порядок по умолчанию).
/// `soulseek` и `trackers` — P2P (desktop-сборка с фичей `p2p`): по умолчанию
/// в конце, при lossless-формате поднимаются наверх (см. `source_order`).
pub const SOURCES: [(&str, &str); 8] = [
    ("vk", "VK"),
    ("yandex", "Яндекс Музыка"),
    ("ytmusic", "YouTube Music"),
    ("soundcloud", "SoundCloud"),
    ("audius", "Audius"),
    ("youtube", "YouTube"),
    ("soulseek", "Soulseek"),
    ("trackers", "Трекеры"),
];

/// P2P-источники доступны только в desktop-сборке с фичей `p2p`.
pub fn p2p_available() -> bool {
    cfg!(feature = "p2p")
}

/// Источник работает от аккаунта пользователя (вошёл и галочка включена).
pub fn active(s: &Settings, kind: &str) -> bool {
    match kind {
        "vk" => s.vk_search && crate::vk::logged_in(),
        "yandex" => s.yandex_acc && crate::yandex_acc::logged_in(),
        "ytmusic" => s.ytm_acc && logged_in("youtube"),
        // P2P «активен», когда включён и настроен: такие источники идут первыми.
        "soulseek" => cfg!(feature = "p2p") && s.p2p_enabled && !s.slsk_username.is_empty() && !s.slsk_password.is_empty(),
        "trackers" => cfg!(feature = "p2p") && s.p2p_enabled,
        _ => false,
    }
}

/// Lossless-формат («как есть» / flac): P2P-источники автоматически поднимаются
/// наверх очереди (soulseek → трекеры), чтобы сначала брался оригинальный FLAC.
pub fn lossless(s: &Settings) -> bool {
    matches!(s.music_format.as_str(), "flac" | "best")
}

/// Порядок из настроек (неизвестное отброшено, недостающее — в конце, как по умолчанию).
pub fn user_order(s: &Settings) -> Vec<&'static str> {
    let mut out: Vec<&'static str> =
        s.music_order.iter().filter_map(|k| SOURCES.iter().find(|(x, _)| x == k).map(|(x, _)| *x)).collect();
    out.dedup();
    for (k, _) in SOURCES {
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

/// Где искать звук: сначала сервисы, куда пользователь вошёл (в его порядке), потом остальные.
/// VK и Яндекс без входа пропускаются — там без аккаунта не ищется.
/// При lossless-формате и включённом P2P: soulseek и трекеры поднимаются в самый
/// верх (soulseek → трекеры) — сначала ищется оригинальный lossless-файл.
pub fn source_order(s: &Settings) -> Vec<&'static str> {
    let mut order = order_with(&user_order(s), |k| active(s, k));
    if lossless(s) && s.p2p_enabled {
        // Вырезаем P2P-источники и ставим их в начало, в порядке soulseek → trackers.
        let mut p2p: Vec<&'static str> = order.iter().copied().filter(|k| matches!(*k, "soulseek" | "trackers")).collect();
        p2p.sort_by_key(|k| if *k == "soulseek" { 0 } else { 1 });
        order.retain(|k| !matches!(*k, "soulseek" | "trackers"));
        let mut out = p2p;
        out.extend(order);
        return out;
    }
    order
}

fn order_with(order: &[&'static str], is_active: impl Fn(&str) -> bool) -> Vec<&'static str> {
    let (mut first, rest): (Vec<_>, Vec<_>) = order.iter().copied().partition(|k| is_active(k));
    first.extend(rest.into_iter().filter(|k| !matches!(*k, "vk" | "yandex")));
    first
}

/// Какой аккаунт с куками относится к ссылке (YouTube / YouTube Music, SoundCloud), если он включён и есть вход.
pub fn for_url(s: &Settings, url: &str) -> Option<(&'static str, PathBuf)> {
    let host = url.split("://").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let key = if host == "youtu.be" || host.ends_with("youtube.com") {
        "youtube"
    } else if host.ends_with("soundcloud.com") || host.ends_with("sndcdn.com") {
        "soundcloud"
    } else {
        return None;
    };
    if !wanted(s, key) {
        return None;
    }
    cookies_file(key).map(|f| (key, f))
}

/// Что даёт вход — для страницы «Аккаунты» (ПК и телефон).
pub struct AccountInfo {
    pub key: &'static str,
    pub name: &'static str,
    /// «звук» — программа берёт звук от аккаунта; «списки» — только названия треков (звук защищён)
    pub audio: bool,
    pub gives: &'static str,
}

pub const ACCOUNTS: [AccountInfo; 10] = [
    AccountInfo { key: "vk", name: "VK", audio: true, gives: "звук прямо из vk (часто mp3 320 и редкие треки), поиск замены там первым" },
    AccountInfo { key: "yandex", name: "Яндекс Музыка", audio: true, gives: "с подпиской плюс — flac (бывает hi-res 24 бит), иначе mp3 320 / aac; поиск замены там" },
    AccountInfo { key: "youtube", name: "YouTube и YouTube Music", audio: true, gives: "с premium — aac 256 kbps вместо opus ~160; 18+ и закрытые видео" },
    AccountInfo { key: "soundcloud", name: "SoundCloud", audio: true, gives: "с go+ — aac 256 kbps и треки go+, которые без входа отдаются отрывком" },
    AccountInfo { key: "spotify", name: "Spotify", audio: false, gives: "свои плейлисты, «любимые треки» и списки больше 100 треков — кнопкой расширения на странице" },
    AccountInfo { key: "deezer", name: "Deezer", audio: false, gives: "свои плейлисты и «любимые треки» — кнопкой расширения на странице" },
    AccountInfo { key: "tidal", name: "Tidal", audio: false, gives: "свои плейлисты и избранное — кнопкой расширения на listen.tidal.com" },
    AccountInfo { key: "qobuz", name: "Qobuz", audio: false, gives: "плейлисты и избранное — кнопкой расширения на play.qobuz.com" },
    AccountInfo { key: "zvuk", name: "Звук", audio: false, gives: "плейлисты и «моя музыка» — кнопкой расширения на zvuk.com" },
    AccountInfo { key: "splice", name: "Splice", audio: false, gives: "название, bpm и тональность сэмпла по ссылке; сами сэмплы — только в приложении splice" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_puts_accounts_first() {
        let order = ["vk", "yandex", "ytmusic", "soundcloud", "audius", "youtube"];
        // Никуда не вошли: VK и Яндекс пропускаются.
        assert_eq!(order_with(&order, |_| false), vec!["ytmusic", "soundcloud", "audius", "youtube"]);
        // Вошли в SoundCloud — он первый.
        assert_eq!(order_with(&order, |k| k == "soundcloud"), vec!["soundcloud", "ytmusic", "audius", "youtube"]);
        // VK и YouTube Music — в порядке пользователя.
        assert_eq!(order_with(&order, |k| k == "vk" || k == "ytmusic"), vec!["vk", "ytmusic", "soundcloud", "audius", "youtube"]);
    }

    #[test]
    fn user_order_fills_missing() {
        let s = Settings { music_order: vec!["youtube".into(), "bogus".into(), "soundcloud".into()], ..Settings::default() };
        assert_eq!(user_order(&s), vec!["youtube", "soundcloud", "vk", "yandex", "ytmusic", "audius", "soulseek", "trackers"]);
    }

    #[test]
    fn lossless_lifts_p2p_to_front() {
        // FLAC + P2P включён: soulseek → трекеры → остальные (в порядке пользователя).
        let s = Settings {
            music_order: vec!["ytmusic".into(), "soulseek".into(), "youtube".into(), "trackers".into()],
            p2p_enabled: true,
            music_format: "flac".into(),
            ..Settings::default()
        };
        assert_eq!(source_order(&s)[0], "soulseek");
        assert_eq!(source_order(&s)[1], "trackers");
        assert_eq!(source_order(&s)[2], "ytmusic");

        // Сжатый формат — автоподъёма нет: soulseek остаётся на месте пользователя
        // (второй), trackers идёт первым лишь потому, что активен без аккаунта —
        // как и другие активные источники. В mp3-режиме P2P в поиске пропускается.
        let s = Settings { music_order: vec!["ytmusic".into(), "soulseek".into(), "youtube".into()], p2p_enabled: true, music_format: "mp3".into(), ..Settings::default() };
        let order = source_order(&s);
        assert_eq!(order.iter().position(|k| *k == "soulseek"), Some(2));
        assert_eq!(order[0], "trackers");
    }

    #[test]
    fn domains() {
        let yt = service("youtube").unwrap();
        assert!(domain_ok(yt, ".youtube.com"));
        assert!(domain_ok(yt, "music.youtube.com"));
        assert!(!domain_ok(yt, "notyoutube.com"));
        assert!(!domain_ok(yt, ".google.com"));
    }

    #[test]
    fn urls() {
        let s = Settings::default();
        assert!(for_url(&s, "https://music.youtube.com/watch?v=x").is_none()); // выключено
    }
}
