//! Музыка: список треков из Spotify, Яндекс Музыки, SoundCloud, YouTube Music и VK.
//!
//! Сервисы делятся на два вида:
//! * звук можно взять прямо с сервиса (SoundCloud, YouTube Music, VK — ссылку на поток
//!   расширение получает на странице вошедшего пользователя) — у трека есть `source`;
//! * сервис отдаёт только список (Spotify, Яндекс Музыка) — звук ищется на YouTube Music,
//!   затем на SoundCloud и YouTube (см. `downloader.rs`). Так же ищется замена треку,
//!   который на своём сервисе заблокирован или удалён.
//!
//! Ограничение «100 треков» снимается постраничной загрузкой: Яндекс отдаёт список страницами,
//! Spotify — через официальный Web API с ключом приложения пользователя (без ключа доступна
//! только встраиваемая страница плеера, а она показывает первые 100 треков).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::appinfo::data_dir;
use crate::settings::Settings;

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub artist: String,
    pub title: String,
    pub album: String,
    /// секунды
    pub duration: Option<f64>,
    pub isrc: Option<String>,
    pub cover: Option<String>,
    /// прямая ссылка на звук (SoundCloud, YouTube Music, поток VK от расширения)
    pub source: Option<String>,
    /// ключ в архиве скачанного, например «spotify:5XuU9htN358NTMCcqRvfDV»
    pub key: String,
}

impl Track {
    pub fn display(&self) -> String {
        match (self.artist.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} — {}", self.artist, self.title),
            (true, false) => self.title.clone(),
            _ => self.source.clone().unwrap_or_else(|| self.key.clone()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MusicList {
    pub name: String,
    pub service: String,
    pub tracks: Vec<Track>,
    /// предупреждение для журнала (например, «показаны первые 100 треков»)
    pub note: Option<String>,
    /// один трек, а не плейлист/альбом
    pub single: bool,
    /// дискография: каждый альбом в своей папке (album трека), номера — по альбому
    pub by_album: bool,
}

/// Строка без ссылки — поиск трека по названию: «исполнитель — название», «название исполнитель» или просто
/// название. Ссылки без `https://` (`youtu.be/…`, `www.site.ru`, `site.com`) поиском не считаются.
pub fn is_search(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.len() > 200 || t.contains('\n') || t.starts_with("vgdlist:") || t.contains("://") {
        return false;
    }
    let first = t.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    let domain_like = |w: &str| {
        let host = w.split('/').next().unwrap_or("");
        host.contains('.')
            && host.rsplit('.').next().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
            && host.split('.').all(|p| !p.is_empty())
    };
    let link = first.starts_with("www.") || (domain_like(&first) && (first.contains('/') || !t.contains(' ')));
    !link && t.chars().any(char::is_alphanumeric)
}

/// Ссылка на музыкальный сервис или поиск трека по названию (идёт в музыкальную загрузку, а не в видео).
pub fn is_music_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    is_search(url)
        || u.starts_with("vgdlist:")
        || u.contains("open.spotify.com/")
        || u.contains("spotify.link/")
        || u.contains("soundcloud.com/")
        || u.contains("music.youtube.com/")
        || u.contains("music.yandex.")
        || u.contains("music.apple.com/")
        || u.contains("deezer.com/")
        || ((u.contains("vk.com/") || u.contains("vk.ru/")) && (u.contains("/music") || u.contains("/audio")))
}

/// Небольшая музыка (трек, альбом или список до 20 треков) — её можно качать параллельно
/// с длинной загрузкой, не дожидаясь очереди.
pub fn is_small_music(url: &str) -> bool {
    if let Some(path) = url.strip_prefix("vgdlist:") {
        return read_list(Path::new(path)).is_ok_and(|l| l.tracks.len() <= 20);
    }
    if !is_music_url(url) {
        return false;
    }
    let u = url.to_ascii_lowercase();
    let path = u.split("://").nth(1).unwrap_or(&u).split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if u.contains("soundcloud.com/") {
        // soundcloud.com/автор/трек — один трек; /sets/, /likes, /tracks и сам автор — нет.
        return segs.len() == 3 && !["sets", "likes", "tracks", "reposts", "albums", "popular-tracks"].contains(&segs[2]);
    }
    if u.contains("music.youtube.com/") {
        return u.contains("watch?v=") && !u.contains("list=");
    }
    if u.contains("vk.com/") || u.contains("vk.ru/") {
        // vk.com/audio-123_456 — один трек; плейлисты и «моя музыка» — нет.
        return segs.last().is_some_and(|s| s.starts_with("audio") && !s.starts_with("audio_playlist") && s.contains('_'));
    }
    // Spotify, Яндекс, Apple, Deezer: трек или альбом.
    segs.iter().any(|s| matches!(*s, "track" | "album" | "song")) || u.contains("?i=")
}

/// Список треков по ссылке. SoundCloud и YouTube Music разворачивает yt-dlp (в `downloader.rs`),
/// поэтому для них здесь `None`.
pub fn resolve(url: &str, s: &Settings, log: &dyn Fn(String)) -> Option<Result<MusicList, String>> {
    let u = url.to_ascii_lowercase();
    if is_search(url) {
        return Some(Ok(search_track(url, log)));
    }
    if let Some(path) = url.strip_prefix("vgdlist:") {
        return Some(read_list(Path::new(path)));
    }
    if u.contains("spotify") {
        return Some(spotify(url, s, log));
    }
    if u.contains("deezer.com/") {
        return Some(deezer(url, log));
    }
    if u.contains("music.apple.com/") {
        if u.contains("/artist/") {
            // Исполнитель Apple Music — вся дискография из каталога Deezer.
            return Some(apple(url).and_then(|l| deezer_discography(&artist_of(&l), log)));
        }
        return Some(apple(url));
    }
    if u.contains("music.yandex.") {
        return Some(yandex(url));
    }
    if u.contains("vk.com/") || u.contains("vk.ru/") {
        return Some(vk_link(url, log));
    }
    None
}

/// Ссылка VK на трек: со входом — сам трек (audio.getById в «невидимом браузере»); без входа — название
/// со страницы и поиск в других местах. Плейлисты VK — расширением браузера (ПК).
fn vk_link(url: &str, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let how = if cfg!(target_os = "android") {
        "Войдите в VK: «Настройки → Аккаунты → войти в VK»."
    } else {
        "Войдите в VK («Настройки → Музыка → войти в VK») или откройте плейлист в браузере, где вы вошли в VK, \
         и нажмите кнопку «скачать плейлист» расширения Very Good Downloader."
    };
    let Some(id) = crate::vk::audio_id(url) else {
        return Err(format!("VK отдаёт плейлисты только после входа в аккаунт. {how}"));
    };
    let single = |t: Track| MusicList { name: t.display(), service: "VK".into(), tracks: vec![t], note: None, by_album: false, single: true };
    if crate::vk::logged_in() {
        match crate::vk::by_id(&id) {
            Ok(hits) if !hits.is_empty() => {
                let h = &hits[0];
                return Ok(single(Track {
                    artist: h.artist.clone(),
                    title: h.title.clone(),
                    duration: h.duration,
                    cover: h.cover.clone(),
                    source: Some(h.url.clone()),
                    key: format!("vk:{id}"),
                    ..Default::default()
                }));
            }
            Ok(_) => log("VK не отдал этот трек (удалён или закрыт) — ищу по названию.".into()),
            Err(e) => log(format!("VK: {e} — ищу по названию.")),
        }
    }
    // Без входа: название со страницы (если VK его показывает), дальше — поиск на других сайтах.
    if let Some((artist, title)) = vk_page_title(url) {
        log(format!("VK без входа: нашёл название «{artist} — {title}», ищу на других сайтах."));
        return Ok(single(Track { artist, title, key: format!("vkname:{id}"), ..Default::default() }));
    }
    Err(format!("VK не показывает трек без входа в аккаунт, а название по ссылке узнать не удалось. {how}"))
}

/// Трек по названию. Сначала каталог Deezer (открытый API, без входа): точные исполнитель, название, альбом,
/// длительность и обложка — по ним загрузчик находит звук на YouTube Music, SoundCloud и других и сверяет его.
/// Deezer не нашёл или недоступен — трек из самой строки («исполнитель — название» или просто название).
fn search_track(q: &str, log: &dyn Fn(String)) -> MusicList {
    let q = q.trim();
    // Первый результат — нужная песня, но часто из сборника; из её записей (то же название и исполнитель)
    // берём самую популярную (`rank`) — обычно это оригинальный альбом.
    let hit = get_json(&format!("https://api.deezer.com/search?q={}&limit=15", qenc(q)), &[]).ok().and_then(|v| {
        let data = v["data"].as_array()?.clone();
        let top = data.first()?;
        let same = |d: &Value| normalize(&s(d, "title")) == normalize(&s(top, "title")) && s(&d["artist"], "name") == s(&top["artist"], "name");
        let best = data.iter().filter(|d| same(d)).max_by_key(|d| d["rank"].as_i64().unwrap_or(0)).unwrap_or(top);
        deezer_track(best, None, None)
    });
    let t = match hit {
        Some(t) => {
            log(format!("Поиск «{q}»: нашёл «{}»{}.", t.display(), if t.album.is_empty() { String::new() } else { format!(", альбом «{}»", t.album) }));
            t
        }
        None => {
            log(format!("Поиск «{q}»: в каталоге Deezer не нашёл — ищу звук прямо по этим словам."));
            let (artist, title) = q
                .split_once(" — ")
                .or_else(|| q.split_once(" – "))
                .or_else(|| q.split_once(" - "))
                .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                .unwrap_or_else(|| (String::new(), q.to_string()));
            Track { artist, title, key: format!("search:{}", q.to_lowercase()), ..Default::default() }
        }
    };
    MusicList { name: t.display(), service: "Поиск".into(), tracks: vec![t], note: None, by_album: false, single: true }
}

/// («исполнитель», «название») из og:title / <title> страницы трека VK, если она открыта без входа.
fn vk_page_title(url: &str) -> Option<(String, String)> {
    let html = fetch(url).ok()?;
    let raw = meta_content(&html, "og:title").or_else(|| {
        let a = html.find("<title>")? + 7;
        Some(html[a..a + html[a..].find("</title>")?].to_string())
    })?;
    page_title_track(&raw)
}

fn page_title_track(raw: &str) -> Option<(String, String)> {
    let t = html_unescape(raw);
    let t = t.trim().trim_end_matches("| ВКонтакте").trim_end_matches("| VK").trim();
    if t.is_empty() || t.contains("ВКонтакте") || t == "VK" {
        return None;
    }
    let (a, b) = t.split_once(" — ").or_else(|| t.split_once(" - "))?;
    Some((a.trim().to_string(), b.trim().to_string()))
}

fn meta_content(html: &str, prop: &str) -> Option<String> {
    let i = html.find(&format!("property=\"{prop}\""))?;
    let tag_start = html[..i].rfind('<')?;
    let tag = &html[tag_start..tag_start + html[tag_start..].find('>')?];
    let c = tag.find("content=\"")? + 9;
    Some(tag[c..c + tag[c..].find('"')?].to_string())
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&#8212;", "—")
}

// ---------- HTTP (crate::http; раньше curl.exe) ----------

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

/// Запрос с теми же настройками, что были у curl: 40 с, 3 повтора, браузерный User-Agent.
fn req(url: &str) -> crate::http::Request {
    crate::http::get(url).ua(UA).timeout(40).retries(3)
}

fn fetch(url: &str) -> Result<String, String> {
    req(url).text()
}

fn get_json(url: &str, headers: &[String]) -> Result<Value, String> {
    let mut r = req(url);
    for h in headers {
        r = r.header_line(h);
    }
    let text = r.text()?;
    serde_json::from_str(&text).map_err(|_| format!("непонятный ответ от {}", host(url)))
}

fn host(url: &str) -> &str {
    crate::http::host(url)
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

// ---------- Spotify ----------

/// (тип, id) из ссылки вида open.spotify.com/intl-ru/playlist/ID?si=…
fn spotify_ref(url: &str) -> Option<(String, String)> {
    let path = url.split("open.spotify.com/").nth(1)?;
    let parts: Vec<&str> = path.split(['?', '#']).next()?.split('/').filter(|p| !p.is_empty()).collect();
    let parts: Vec<&str> = parts.into_iter().filter(|p| !p.starts_with("intl-") && *p != "embed").collect();
    let kind = parts.first()?.to_string();
    let id = parts.get(1)?.to_string();
    matches!(kind.as_str(), "playlist" | "album" | "track" | "artist").then_some((kind, id))
}

fn spotify(url: &str, st: &Settings, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let mut url = url.to_string();
    if url.contains("spotify.link/") {
        // короткая ссылка из приложения → полная
        if let Ok(u) = req(&url).retries(0).final_url() {
            url = u;
        }
    }
    let (kind, id) = spotify_ref(&url).ok_or("не понял ссылку Spotify")?;
    if kind == "artist" {
        // Исполнитель — вся дискография: имя со страницы плеера, альбомы из каталога Deezer.
        let l = spotify_embed(&kind, &id)?;
        let name = if l.name.is_empty() { artist_of(&l) } else { l.name.clone() };
        return deezer_discography(&name, log);
    }
    if kind == "playlist" {
        return spotify_playlist(&id, log);
    }
    // Ключ — только альбомы и треки: с февраля 2026 Spotify Web API в Development Mode не отдаёт содержимое
    // чужих плейлистов (GET /playlists/{id}/items — только свои), top-tracks исполнителя удалён.
    if !st.spotify_id.trim().is_empty() && !st.spotify_secret.trim().is_empty() {
        match spotify_api(&kind, &id, st) {
            Ok(l) => return Ok(l),
            Err(e) => {
                log(format!("Внимание: ключ Spotify не сработал: {e}"));
                let mut l = spotify_embed(&kind, &id)?;
                l.note = Some(format!(
                    "Ключ Spotify не сработал ({e}). {}",
                    if l.tracks.len() >= 100 { "Без него видны только первые 100 треков." } else { "Список взят без ключа." }
                ));
                return Ok(l);
            }
        }
    }
    spotify_embed(&kind, &id)
}

/// Страница плейлиста Spotify, которую открывает программа (на Android — скрытый WebView): прокручивает список
/// и отдаёт все треки JSON-ом `{name, total, tracks: [{id, title, artist, album, duration, cover}]}`.
/// На ПК весь плейлист передаёт расширение браузера.
/// Код для страницы плейлиста (прокрутка и сбор треков) — для «невидимого браузера» программы.
pub const SPOTIFY_AGENT_JS: &str = include_str!("spotify_agent.js");

pub trait SpotifyPage: Send + Sync {
    fn playlist(&self, id: &str) -> Result<String, String>;
}

static SPOTIFY_PAGE: std::sync::RwLock<Option<std::sync::Arc<dyn SpotifyPage>>> = std::sync::RwLock::new(None);

pub fn set_spotify_page(p: std::sync::Arc<dyn SpotifyPage>) {
    *SPOTIFY_PAGE.write().unwrap_or_else(|e| e.into_inner()) = Some(p);
}

fn spotify_page() -> Option<std::sync::Arc<dyn SpotifyPage>> {
    SPOTIFY_PAGE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Как получить весь плейлист, если программе видны только первые 100 треков.
fn spotify_full_hint() -> &'static str {
    if cfg!(target_os = "android") {
        "Spotify отдал программе только первые 100 треков, а собрать весь список со страницы не вышло — скачаю первые 100."
    } else {
        "Spotify отдаёт программе только первые 100 треков. Чтобы скачать плейлист целиком, откройте его в браузере \
         и нажмите кнопку расширения «скачать плейлист»."
    }
}

/// Плейлист: встраиваемый плеер (до 100 треков); если треков ровно 100 и есть страница-агент — весь список с неё.
fn spotify_playlist(id: &str, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let embed = spotify_embed("playlist", id);
    let cut = embed.as_ref().map_or(true, |l| l.tracks.len() >= 100);
    if cut {
        if let Some(page) = spotify_page() {
            log("Spotify: собираю весь плейлист со страницы open.spotify.com…".into());
            match page.playlist(id).and_then(|j| tracks_from_page(&j)) {
                Ok(l) if !l.tracks.is_empty() => {
                    log(format!("Spotify: со страницы — {} трек(ов).", l.tracks.len()));
                    return Ok(l);
                }
                Ok(_) => log("Внимание: Spotify: на странице плейлиста нет треков.".into()),
                Err(e) => log(format!("Внимание: Spotify: страница плейлиста: {e}")),
            }
        }
    }
    let mut l = embed?;
    if l.tracks.len() >= 100 {
        l.note = Some(spotify_full_hint().into());
    }
    Ok(l)
}

/// Ответ страницы-агента → список треков.
fn tracks_from_page(json: &str) -> Result<MusicList, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "непонятный ответ страницы".to_string())?;
    if let Some(e) = v["error"].as_str() {
        return Err(e.to_string());
    }
    let tracks: Vec<Track> = v["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let title = s(t, "title").trim().to_string();
            (!title.is_empty()).then(|| Track {
                key: format!("spotify:{}", s(t, "id")),
                artist: s(t, "artist").trim().to_string(),
                title,
                album: s(t, "album"),
                duration: t["duration"].as_f64(),
                cover: t["cover"].as_str().filter(|c| c.starts_with("http")).map(String::from),
                ..Default::default()
            })
        })
        .collect();
    let total = v["total"].as_u64().unwrap_or(0) as usize;
    let note = (total > tracks.len()).then(|| format!("Со страницы Spotify собрано {} из {total} треков.", tracks.len()));
    Ok(MusicList { name: s(&v, "name").if_empty("Spotify"), service: "Spotify".into(), tracks, note, by_album: false, single: false })
}

/// Встраиваемый плеер: без ключей, но не больше 100 треков.
fn spotify_embed(kind: &str, id: &str) -> Result<MusicList, String> {
    let r = req(&format!("https://open.spotify.com/embed/{kind}/{id}")).header("Accept-Language", "en-US,en;q=0.9").send()?;
    let html = String::from_utf8_lossy(&r.body).into_owned();
    let Some(start) = html.find("<script id=\"__NEXT_DATA__\"") else {
        // Бывает с некоторых адресов (VPN): вместо плеера — другая страница. Код и размер — для разбора по журналу.
        let page = html.find("<title>").and_then(|a| html[a + 7..].find("</title>").map(|b| html[a + 7..a + 7 + b].trim().to_string()));
        let why = format!(
            "Spotify не отдал страницу плеера (HTTP {}, {} байт{})",
            r.status,
            r.body.len(),
            page.filter(|t| !t.is_empty()).map(|t| format!(", «{}»", t.chars().take(60).collect::<String>())).unwrap_or_default()
        );
        // Трек: название — из oEmbed Spotify, дальше обычный поиск по сайтам.
        if kind == "track" {
            if let Some(t) = spotify_oembed_title(id) {
                return Ok(MusicList {
                    name: t.clone(),
                    service: "Spotify".into(),
                    tracks: vec![Track { title: t, key: format!("spotify:{id}"), ..Default::default() }],
                    note: Some(format!("{why}; ищу по названию трека.")),
                    by_album: false,
                    single: true,
                });
            }
        }
        return Err(why);
    };
    let body_start = html[start..].find('>').map(|i| start + i + 1).ok_or("Spotify: нет данных")?;
    let end = html[body_start..].find("</script>").map(|i| body_start + i).ok_or("Spotify: нет данных")?;
    let d: Value = serde_json::from_str(&html[body_start..end]).map_err(|_| "Spotify: непонятные данные")?;
    let e = &d["props"]["pageProps"]["state"]["data"]["entity"];
    if e.is_null() {
        return Err("Spotify: плейлист не найден или закрыт".into());
    }
    let name = s(e, "name").to_string();
    let entity_cover = e["coverArt"]["sources"].as_array().and_then(|a| a.first()).map(|c| s(c, "url"));
    let mut tracks = Vec::new();
    if kind == "track" {
        tracks.push(Track {
            artist: e["artists"].as_array().map(|a| a.iter().map(|x| s(x, "name")).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
            title: name.clone(),
            duration: e["duration"].as_f64().map(|ms| ms / 1000.0),
            cover: entity_cover.clone(),
            key: format!("spotify:{id}"),
            ..Default::default()
        });
    } else if let Some(list) = e["trackList"].as_array() {
        for t in list {
            let tid = s(t, "uri").rsplit(':').next().unwrap_or("").to_string();
            tracks.push(Track {
                artist: s(t, "subtitle").replace('\u{a0}', " "),
                title: s(t, "title"),
                album: if kind == "album" { name.clone() } else { String::new() },
                duration: t["duration"].as_f64().map(|ms| ms / 1000.0),
                cover: if kind == "album" { entity_cover.clone() } else { None },
                key: format!("spotify:{tid}"),
                ..Default::default()
            });
        }
    }
    Ok(MusicList { name, service: "Spotify".into(), tracks, note: None, by_album: false, single: kind == "track" })
}

/// Название трека из oEmbed (официальный ответ для вставки плеера; исполнителя там нет).
fn spotify_oembed_title(id: &str) -> Option<String> {
    let v = get_json(&format!("https://open.spotify.com/oembed?url=https://open.spotify.com/track/{id}"), &[]).ok()?;
    Some(s(&v, "title")).filter(|t| !t.is_empty())
}

fn spotify_token(st: &Settings) -> Result<String, String> {
    let text = crate::http::post("https://accounts.spotify.com/api/token")
        .ua(UA)
        .timeout(40)
        .retries(3)
        .basic_auth(st.spotify_id.trim(), st.spotify_secret.trim())
        .form(&[("grant_type", "client_credentials")])
        .text()?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "непонятный ответ")?;
    v["access_token"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("ключ не подошёл ({})", s(&v, "error_description").if_empty(&s(&v, "error"))))
}

trait IfEmpty {
    fn if_empty(self, o: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, o: &str) -> String {
        if self.is_empty() { o.to_string() } else { self }
    }
}

fn spotify_track(t: &Value, album_name: Option<&str>, album_cover: Option<&String>) -> Option<Track> {
    let id = t["id"].as_str()?;
    let cover = t["album"]["images"].as_array().and_then(|a| a.first()).map(|i| s(i, "url")).or_else(|| album_cover.cloned());
    Some(Track {
        artist: t["artists"].as_array().map(|a| a.iter().map(|x| s(x, "name")).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        title: s(t, "name"),
        album: album_name.map(String::from).unwrap_or_else(|| s(&t["album"], "name")),
        duration: t["duration_ms"].as_f64().map(|ms| ms / 1000.0),
        isrc: t["external_ids"]["isrc"].as_str().map(String::from),
        cover,
        source: None,
        key: format!("spotify:{id}"),
    })
}

/// Официальный Web API (ключ приложения пользователя): список целиком, страницами по 100.
fn spotify_api(kind: &str, id: &str, st: &Settings) -> Result<MusicList, String> {
    let token = spotify_token(st)?;
    let h = vec![format!("Authorization: Bearer {token}")];
    let api = "https://api.spotify.com/v1";
    let mut tracks = Vec::new();
    let (name, single) = match kind {
        "track" => {
            let t = get_json(&format!("{api}/tracks/{id}"), &h)?;
            tracks.extend(spotify_track(&t, None, None));
            (s(&t, "name"), true)
        }
        "album" => {
            let a = get_json(&format!("{api}/albums/{id}"), &h)?;
            let name = s(&a, "name");
            let cover = a["images"].as_array().and_then(|x| x.first()).map(|i| s(i, "url"));
            let mut next = Some(format!("{api}/albums/{id}/tracks?limit=50"));
            while let Some(u) = next {
                let page = get_json(&u, &h)?;
                for t in page["items"].as_array().into_iter().flatten() {
                    tracks.extend(spotify_track(t, Some(&name), cover.as_ref()));
                }
                next = page["next"].as_str().map(String::from);
            }
            (name, false)
        }
        // Плейлисты и исполнители через API больше не берём (февраль 2026): плейлист — плеер и страница,
        // исполнитель — дискография из Deezer.
        _ => return Err("плейлисты и исполнители Spotify через ключ недоступны".into()),
    };
    Ok(MusicList { name, service: "Spotify".into(), tracks, note: None, by_album: false, single })
}

// ---------- Apple Music (iTunes Lookup API и данные страницы плейлиста; звука не отдаёт) ----------

fn apple_art(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    // «…/100x100bb.jpg» или шаблон «…/{w}x{h}{c}.{f}» → 600×600
    let base = match url.rfind('/') {
        Some(i) => &url[..i],
        None => url,
    };
    Some(format!("{base}/600x600bb.jpg"))
}

fn apple_lookup_track(t: &Value) -> Option<Track> {
    if s(t, "wrapperType") != "track" {
        return None;
    }
    let id = t["trackId"].as_i64()?;
    Some(Track {
        artist: s(t, "artistName"),
        title: s(t, "trackName"),
        album: s(t, "collectionName"),
        duration: t["trackTimeMillis"].as_f64().map(|ms| ms / 1000.0),
        isrc: None,
        cover: apple_art(&s(t, "artworkUrl100")),
        source: None,
        key: format!("apple:{id}"),
    })
}

fn apple(url: &str) -> Result<MusicList, String> {
    let rest = url.split("music.apple.com/").nth(1).ok_or("не понял ссылку Apple Music")?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let p: Vec<&str> = path.split(['/', '#']).filter(|x| !x.is_empty()).collect();
    // /us/album/название/123 · /us/song/название/123 · /us/playlist/название/pl.xxx · /us/artist/название/123
    let (cc, kind) = match p.as_slice() {
        [cc, kind, ..] if cc.len() == 2 => (cc.to_string(), kind.to_string()),
        [kind, ..] => ("us".to_string(), kind.to_string()),
        _ => return Err("не понял ссылку Apple Music".into()),
    };
    let id = p.last().copied().unwrap_or("").to_string();
    let song_in_album = query.split('&').find_map(|kv| kv.strip_prefix("i=")).map(String::from);
    let lookup = |q: &str| get_json(&format!("https://itunes.apple.com/lookup?{q}&country={cc}"), &[]);
    let arr = |v: &Value| v["results"].as_array().cloned().unwrap_or_default();
    match kind.as_str() {
        "song" | "album" if kind == "song" || song_in_album.is_some() => {
            let tid = song_in_album.unwrap_or(id);
            let v = lookup(&format!("id={tid}"))?;
            let tracks: Vec<Track> = arr(&v).iter().filter_map(apple_lookup_track).collect();
            let name = tracks.first().map(|t| t.display()).unwrap_or_default();
            Ok(MusicList { name, service: "Apple Music".into(), tracks, note: None, by_album: false, single: true })
        }
        "album" => {
            let v = lookup(&format!("id={id}&entity=song&limit=200"))?;
            let r = arr(&v);
            let name = r.iter().find(|x| s(x, "wrapperType") == "collection").map(|x| s(x, "collectionName")).unwrap_or_default();
            let tracks = r.iter().filter_map(apple_lookup_track).collect();
            Ok(MusicList { name, service: "Apple Music".into(), tracks, note: None, by_album: false, single: false })
        }
        "artist" => {
            let v = lookup(&format!("id={id}&entity=song&limit=200"))?;
            let r = arr(&v);
            let name = r.iter().find(|x| s(x, "wrapperType") == "artist").map(|x| s(x, "artistName")).unwrap_or_default();
            let tracks = r.iter().filter_map(apple_lookup_track).collect();
            Ok(MusicList { name, service: "Apple Music".into(), tracks, note: None, by_album: false, single: false })
        }
        "playlist" => apple_playlist(url),
        _ => Err("Apple Music: поддерживаются трек, альбом, плейлист и исполнитель".into()),
    }
}

/// Плейлист: список лежит в данных страницы (`serialized-server-data`).
fn apple_playlist(url: &str) -> Result<MusicList, String> {
    let html = fetch(url)?;
    let start = html.find("id=\"serialized-server-data\"").ok_or("Apple Music не отдал страницу плейлиста")?;
    let body = html[start..].find('>').map(|i| start + i + 1).ok_or("Apple Music: нет данных")?;
    let end = html[body..].find("</script>").map(|i| body + i).ok_or("Apple Music: нет данных")?;
    let d: Value = serde_json::from_str(&html[body..end]).map_err(|_| "Apple Music: непонятные данные")?;
    let sections = d["data"][0]["data"]["sections"].as_array().cloned().unwrap_or_default();
    let mut name = String::new();
    let mut tracks = Vec::new();
    for sec in &sections {
        let items = sec["items"].as_array().cloned().unwrap_or_default();
        if s(sec, "itemKind") == "containerDetailHeaderLockup" {
            if let Some(h) = items.first() {
                name = s(h, "title");
            }
        }
        if s(sec, "itemKind") != "trackLockup" {
            continue;
        }
        for it in &items {
            let id = s(&it["contentDescriptor"]["identifiers"], "storeAdamID");
            let artist = s(it, "artistName");
            let artist = if artist.is_empty() {
                it["subtitleLinks"].as_array().map(|a| a.iter().map(|x| s(x, "title")).collect::<Vec<_>>().join(", ")).unwrap_or_default()
            } else {
                artist
            };
            tracks.push(Track {
                artist,
                title: s(it, "title"),
                album: it["tertiaryLinks"].as_array().and_then(|a| a.first()).map(|x| s(x, "title")).unwrap_or_default(),
                duration: it["duration"].as_f64().map(|ms| ms / 1000.0),
                isrc: None,
                cover: apple_art(&s(&it["artwork"]["dictionary"], "url")),
                source: None,
                key: format!("apple:{}", if id.is_empty() { s(it, "title") } else { id }),
            });
        }
    }
    let note = (tracks.len() >= 100).then(|| {
        "Apple Music отдаёт программе только начало большого плейлиста. Чтобы скачать его целиком, откройте плейлист \
         в браузере и нажмите кнопку расширения ↓."
            .to_string()
    });
    Ok(MusicList { name: if name.is_empty() { "Apple Music".into() } else { name }, service: "Apple Music".into(), tracks, note, by_album: false, single: false })
}

// ---------- Яндекс Музыка (открытый API: только списки, без звука) ----------

fn yandex_track(t: &Value) -> Option<Track> {
    let id = t["id"].as_str().map(String::from).or_else(|| t["id"].as_i64().map(|i| i.to_string()))?;
    let mut title = s(t, "title");
    let version = s(t, "version");
    if !version.is_empty() {
        title = format!("{title} ({version})");
    }
    let cover = t["coverUri"].as_str().filter(|c| !c.is_empty()).map(|c| format!("https://{}", c.replace("%%", "600x600")));
    Some(Track {
        artist: t["artists"].as_array().map(|a| a.iter().map(|x| s(x, "name")).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        title,
        album: t["albums"].as_array().and_then(|a| a.first()).map(|a| s(a, "title")).unwrap_or_default(),
        duration: t["durationMs"].as_f64().map(|ms| ms / 1000.0),
        isrc: None,
        cover,
        source: None,
        key: format!("yandex:{id}"),
    })
}

fn yandex(url: &str) -> Result<MusicList, String> {
    let api = "https://api.music.yandex.net";
    let path = url.split("music.yandex.").nth(1).and_then(|r| r.split_once('/')).map(|(_, p)| p).unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("");
    let p: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
    // С входом в Яндекс — от аккаунта: без него API иногда отвечает «Unavailable For Legal Reasons» (VPN, страна).
    let none: Vec<String> = crate::yandex_acc::token().map(|t| vec![format!("Authorization: OAuth {t}")]).unwrap_or_default();
    let arr = |v: &Value| -> Vec<Value> { v.as_array().cloned().unwrap_or_default() };
    match p.as_slice() {
        ["album", _, "track", tid, ..] | ["track", tid, ..] => {
            let v = get_json(&format!("{api}/tracks/{tid}"), &none)?;
            let tracks: Vec<Track> = arr(&v["result"]).iter().filter_map(yandex_track).collect();
            let name = tracks.first().map(|t| t.display()).unwrap_or_default();
            Ok(MusicList { name, service: "Яндекс Музыка".into(), tracks, note: None, by_album: false, single: true })
        }
        ["album", aid, ..] => {
            let v = get_json(&format!("{api}/albums/{aid}/with-tracks"), &none)?;
            let r = &v["result"];
            let tracks = arr(&r["volumes"]).iter().flat_map(|vol| arr(vol)).filter_map(|t| yandex_track(&t)).collect();
            Ok(MusicList { name: s(r, "title"), service: "Яндекс Музыка".into(), tracks, note: None, by_album: false, single: false })
        }
        ["artist", aid, ..] => {
            let info = get_json(&format!("{api}/artists/{aid}/brief-info"), &none)?;
            let name = s(&info["result"]["artist"], "name");
            let mut tracks = Vec::new();
            let mut page = 0;
            loop {
                let v = get_json(&format!("{api}/artists/{aid}/tracks?page={page}&page-size=100"), &none)?;
                let got = arr(&v["result"]["tracks"]);
                if got.is_empty() {
                    break;
                }
                tracks.extend(got.iter().filter_map(yandex_track));
                let total = v["result"]["pager"]["total"].as_u64().unwrap_or(0) as usize;
                page += 1;
                if tracks.len() >= total || page > 100 {
                    break;
                }
            }
            Ok(MusicList { name, service: "Яндекс Музыка".into(), tracks, note: None, by_album: false, single: false })
        }
        ["users", user, "playlists", kind, ..] => {
            let v = get_json(&format!("{api}/users/{user}/playlists/{kind}?rich-tracks=true"), &none)?;
            yandex_playlist(&v["result"])
        }
        ["playlists", uuid, ..] => {
            let v = get_json(&format!("{api}/playlist/{uuid}?rich-tracks=true"), &none)?;
            yandex_playlist(&v["result"])
        }
        _ => Err("не понял ссылку Яндекс Музыки (нужен трек, альбом, артист или плейлист)".into()),
    }
}

fn yandex_playlist(r: &Value) -> Result<MusicList, String> {
    if r.is_null() {
        return Err("Яндекс Музыка: плейлист не найден или закрыт".into());
    }
    let tracks = r["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| yandex_track(if it["track"].is_object() { &it["track"] } else { it }))
        .collect();
    Ok(MusicList { name: s(r, "title"), service: "Яндекс Музыка".into(), tracks, note: None, by_album: false, single: false })
}

// ---------- список из расширения браузера (VK) ----------

/// Сохранить список треков, присланный расширением, и вернуть ссылку `vgdlist:` для очереди.
/// `diag` — что расширение смогло узнать на странице (для разбора, если звук не нашёлся).
pub fn save_list(name: &str, service: &str, tracks: &Value, diag: Option<&Value>) -> Option<String> {
    let dir = data_dir().join("lists");
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    let path = dir.join(format!("{stamp}.json"));
    let body = json!({ "name": name, "service": service, "tracks": tracks, "diag": diag });
    std::fs::write(&path, body.to_string()).ok()?;
    Some(format!("vgdlist:{}", path.display()))
}

fn read_list(path: &Path) -> Result<MusicList, String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|_| "список треков повреждён")?;
    let service = s(&v, "service").if_empty("VK");
    let prefix = service.to_lowercase();
    let tracks: Vec<Track> = v["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let artist = s(t, "artist").trim().to_string();
            let title = s(t, "title").trim().to_string();
            if title.is_empty() {
                return None;
            }
            let id = s(t, "id").if_empty(&format!("{artist} - {title}").to_lowercase());
            Some(Track {
                key: format!("{prefix}:{id}"),
                artist,
                title,
                album: s(t, "album"),
                duration: t["duration"].as_f64(),
                isrc: None,
                cover: t["cover"].as_str().filter(|c| c.starts_with("http")).map(String::from),
                // ссылка на звук (HLS), которую расширение получило на странице VK
                source: t["url"].as_str().filter(|u| u.starts_with("http")).map(String::from),
            })
        })
        .collect();
    let single = tracks.len() == 1;
    Ok(MusicList { name: s(&v, "name").if_empty("VK"), service, tracks, note: None, by_album: false, single })
}

// ---------- дополнительные источники для поиска ----------

fn qenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// ISRC трека по названию через открытый API Deezer: по ISRC YouTube Music находит
/// ровно ту запись (ту же версию, тот же мастер), а не кавер или ремикс.
pub fn deezer_isrc(t: &Track) -> Option<String> {
    let artist = t.artist.split(',').next().unwrap_or("").trim();
    let queries = [format!("artist:\"{artist}\" track:\"{}\"", t.title), format!("{artist} {}", t.title)];
    for q in queries {
        let v = get_json(&format!("https://api.deezer.com/search?q={}&limit=5", qenc(&q)), &[]).ok()?;
        for d in v["data"].as_array().into_iter().flatten() {
            let id = d["id"].as_i64()?;
            let dur = d["duration"].as_f64();
            let title = s(d, "title");
            let art = s(&d["artist"], "name");
            if !matches(t, &title, &art, dur) {
                continue;
            }
            // В поиске ISRC нет — берём из карточки трека.
            let full = get_json(&format!("https://api.deezer.com/track/{id}"), &[]).ok()?;
            if let Some(isrc) = full["isrc"].as_str().filter(|x| !x.is_empty()) {
                return Some(isrc.to_string());
            }
        }
    }
    None
}

/// ISRC трека Deezer по его id.
/// 30-секундный отрывок трека с Deezer (mp3) — образец для проверки по звуку.
/// По ISRC, по id трека Deezer или поиском по исполнителю и названию.
pub fn deezer_preview(t: &Track) -> Option<String> {
    let pick = |v: &Value| v["preview"].as_str().filter(|x| !x.is_empty()).map(String::from);
    if let Some(isrc) = &t.isrc {
        if let Some(p) = get_json(&format!("https://api.deezer.com/track/isrc:{isrc}"), &[]).ok().as_ref().and_then(pick) {
            return Some(p);
        }
    }
    if let Some(id) = t.key.strip_prefix("deezer:") {
        if let Some(p) = get_json(&format!("https://api.deezer.com/track/{id}"), &[]).ok().as_ref().and_then(pick) {
            return Some(p);
        }
    }
    let artist = t.artist.split(',').next().unwrap_or("").trim();
    if artist.is_empty() || t.title.is_empty() {
        return None;
    }
    let q = format!("artist:\"{artist}\" track:\"{}\"", t.title);
    let v = get_json(&format!("https://api.deezer.com/search?q={}&limit=5", qenc(&q)), &[]).ok()?;
    v["data"]
        .as_array()?
        .iter()
        .find(|d| matches(t, &s(d, "title"), &s(&d["artist"], "name"), d["duration"].as_f64()))
        .and_then(pick)
}

/// Тот же трек на других сервисах через song.link (Odesli): точное совпадение, без поиска по названию.
/// Порядок: YouTube Music, SoundCloud, YouTube. С октября 2026 открытый API требует ключ (`key`);
/// без ключа или после отказа — сразу пусто (обычный поиск). Не чаще раза в 6,5 с (лимит 10 в минуту).
pub fn odesli(t: &Track, key: &str) -> Vec<(&'static str, String)> {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    static REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if key.trim().is_empty() || REFUSED.load(std::sync::atomic::Ordering::Relaxed) {
        return Vec::new();
    }
    let (platform, id) = match t.key.split_once(':') {
        Some(("spotify", id)) => ("spotify", id),
        Some(("deezer", id)) => ("deezer", id),
        Some(("yandex", id)) => ("yandex", id),
        Some(("apple", id)) if id.chars().all(|c| c.is_ascii_digit()) => ("appleMusic", id),
        _ => return Vec::new(),
    };
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|l| l.elapsed() < std::time::Duration::from_millis(6500)) {
            return Vec::new();
        }
        *last = Some(std::time::Instant::now());
    }
    let url = format!("https://api.song.link/v1-alpha.1/links?platform={platform}&type=song&id={}&userCountry=RU&key={}", qenc(id), qenc(key.trim()));
    let Ok(v) = get_json(&url, &[]) else { return Vec::new() };
    if v["statusCode"].as_i64().is_some_and(|c| c == 401 || c == 403) {
        REFUSED.store(true, std::sync::atomic::Ordering::Relaxed); // ключ не подошёл — больше не спрашиваем
        return Vec::new();
    }
    let links = &v["linksByPlatform"];
    [("youtubeMusic", "YouTube Music"), ("soundcloud", "SoundCloud"), ("youtube", "YouTube")]
        .iter()
        .filter_map(|(k, label)| links[*k]["url"].as_str().map(|u| (*label, u.to_string())))
        .collect()
}

/// Скачать файл по ссылке.
pub fn download_to(url: &str, path: &Path) -> Result<(), String> {
    req(url).download(path)?;
    if path.metadata().map(|m| m.len()).unwrap_or(0) < 1000 {
        return Err("файл не скачался".into());
    }
    Ok(())
}

pub fn deezer_track_isrc(id: &str) -> Option<String> {
    let v = get_json(&format!("https://api.deezer.com/track/{id}"), &[]).ok()?;
    v["isrc"].as_str().filter(|x| !x.is_empty()).map(String::from)
}

/// Audius — открытая площадка музыкантов (много электроники, ремиксов и треков, которых нет
/// на YouTube/SoundCloud). Отдаёт полный mp3 без входа. (ссылка на звук, название, автор, длительность)
pub fn audius_search(q: &str) -> Result<Vec<(String, String, String, Option<f64>)>, String> {
    let v = get_json(&format!("https://api.audius.co/v1/tracks/search?query={}&app_name=very-good-downloader", qenc(q)), &[])?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["is_streamable"].as_bool().unwrap_or(true))
        .take(4)
        .filter_map(|t| {
            let id = t["id"].as_str()?;
            Some((
                format!("https://api.audius.co/v1/tracks/{id}/stream?app_name=very-good-downloader"),
                s(t, "title"),
                s(&t["user"], "name"),
                t["duration"].as_f64(),
            ))
        })
        .collect())
}

/// Текст песни с LRCLIB (открытая база, без ключа): (синхронный .lrc, обычный текст).
pub fn lrclib(t: &Track) -> Option<(Option<String>, Option<String>)> {
    let artist = t.artist.split(',').next().unwrap_or("").trim();
    if artist.is_empty() || t.title.is_empty() {
        return None;
    }
    let mut url = format!("https://lrclib.net/api/get?artist_name={}&track_name={}", qenc(artist), qenc(&t.title));
    if !t.album.is_empty() {
        url += &format!("&album_name={}", qenc(&t.album));
    }
    if let Some(d) = t.duration {
        url += &format!("&duration={}", d.round() as i64);
    }
    let pick = |v: &Value| -> Option<(Option<String>, Option<String>)> {
        if v["instrumental"].as_bool() == Some(true) {
            return None;
        }
        let synced = v["syncedLyrics"].as_str().filter(|x| !x.trim().is_empty()).map(String::from);
        let plain = v["plainLyrics"].as_str().filter(|x| !x.trim().is_empty()).map(String::from);
        (synced.is_some() || plain.is_some()).then_some((synced, plain))
    };
    if let Ok(v) = get_json(&url, &[]) {
        if let Some(r) = pick(&v) {
            return Some(r);
        }
    }
    // Точного совпадения нет — поиск, с проверкой длительности.
    let q = format!("https://lrclib.net/api/search?track_name={}&artist_name={}", qenc(&t.title), qenc(artist));
    let v = get_json(&q, &[]).ok()?;
    v.as_array()?.iter().find_map(|x| {
        let ok = match (t.duration, x["duration"].as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 3.0,
            _ => true,
        };
        if ok { pick(x) } else { None }
    })
}

fn artist_of(l: &MusicList) -> String {
    let a = l.tracks.first().map(|t| t.artist.split(',').next().unwrap_or("").trim().to_string()).unwrap_or_default();
    if a.is_empty() { l.name.clone() } else { a }
}

/// Ссылки Deezer: альбом, плейлист, трек, исполнитель (дискография) и «поиск исполнителя» от расширения.
fn deezer(url: &str, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let path = url.split("deezer.com/").nth(1).unwrap_or("");
    let (path, frag) = path.split_once('#').unwrap_or((path, ""));
    let p: Vec<&str> = path.split(['/', '?']).filter(|x| !x.is_empty()).collect();
    let find = |kind: &str| p.iter().position(|x| *x == kind).and_then(|i| p.get(i + 1)).map(|x| x.to_string());
    if frag.contains("vgd-discography") || p.contains(&"search") {
        let name = p.last().map(|x| percent_decode_str(x)).unwrap_or_default();
        return deezer_discography(&name, log);
    }
    if let Some(id) = find("artist") {
        return deezer_discography_id(id.parse().map_err(|_| "не понял ссылку Deezer")?, log);
    }
    let api = "https://api.deezer.com";
    if let Some(id) = find("album") {
        let a = get_json(&format!("{api}/album/{id}"), &[])?;
        let tracks = deezer_album_tracks(&a);
        return Ok(MusicList { name: s(&a, "title"), service: "Deezer".into(), tracks, note: None, by_album: false, single: false });
    }
    if let Some(id) = find("playlist") {
        let pl = get_json(&format!("{api}/playlist/{id}"), &[])?;
        let mut tracks = Vec::new();
        let mut next = Some(format!("{api}/playlist/{id}/tracks?limit=100"));
        while let Some(u) = next {
            let page = get_json(&u, &[])?;
            tracks.extend(page["data"].as_array().into_iter().flatten().filter_map(|t| deezer_track(t, None, None)));
            next = page["next"].as_str().map(String::from);
        }
        return Ok(MusicList { name: s(&pl, "title"), service: "Deezer".into(), tracks, note: None, by_album: false, single: false });
    }
    if let Some(id) = find("track") {
        let t = get_json(&format!("{api}/track/{id}"), &[])?;
        let tracks: Vec<Track> = deezer_track(&t, None, None).into_iter().collect();
        let name = tracks.first().map(|t| t.display()).unwrap_or_default();
        return Ok(MusicList { name, service: "Deezer".into(), tracks, note: None, by_album: false, single: true });
    }
    Err("не понял ссылку Deezer".into())
}

fn percent_decode_str(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn deezer_track(t: &Value, album: Option<&str>, cover: Option<&String>) -> Option<Track> {
    let id = t["id"].as_i64()?;
    if t["readable"].as_bool() == Some(false) && t["title"].is_null() {
        return None;
    }
    let mut title = s(t, "title");
    let ver = s(t, "title_version");
    if !ver.is_empty() && !title.contains(&ver) {
        title = format!("{title} {ver}");
    }
    Some(Track {
        artist: s(&t["artist"], "name"),
        title,
        album: album.map(String::from).unwrap_or_else(|| s(&t["album"], "title")),
        duration: t["duration"].as_f64(),
        isrc: t["isrc"].as_str().filter(|x| !x.is_empty()).map(String::from),
        cover: cover.cloned().or_else(|| t["album"]["cover_xl"].as_str().map(String::from)),
        source: None,
        key: format!("deezer:{id}"),
    })
}

fn deezer_album_tracks(a: &Value) -> Vec<Track> {
    let album = s(a, "title");
    let cover = a["cover_xl"].as_str().map(String::from);
    a["tracks"]["data"].as_array().into_iter().flatten().filter_map(|t| deezer_track(t, Some(&album), cover.as_ref())).collect()
}

/// Дискография по имени исполнителя (каталог Deezer — полный, с ISRC и обложками).
pub fn deezer_discography(name: &str, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("не понял, какой исполнитель".into());
    }
    let v = get_json(&format!("https://api.deezer.com/search/artist?q={}&limit=10", qenc(name)), &[])?;
    let want = normalize(name);
    let list = v["data"].as_array().cloned().unwrap_or_default();
    // Сначала точное совпадение имени (из самых популярных), иначе — первый в выдаче.
    let pick = list.iter().find(|a| normalize(&s(a, "name")) == want).or(list.first()).ok_or("исполнитель не найден в каталоге")?;
    deezer_discography_id(pick["id"].as_i64().unwrap_or(0), log)
}

fn deezer_discography_id(id: i64, log: &dyn Fn(String)) -> Result<MusicList, String> {
    let api = "https://api.deezer.com";
    let artist = get_json(&format!("{api}/artist/{id}"), &[])?;
    let name = s(&artist, "name");
    let mut albums = Vec::new();
    let mut next = Some(format!("{api}/artist/{id}/albums?limit=100"));
    while let Some(u) = next {
        let page = get_json(&u, &[])?;
        albums.extend(page["data"].as_array().cloned().unwrap_or_default());
        next = page["next"].as_str().map(String::from);
    }
    // Одинаковые альбомы (обычная и «чистая» версии) — один раз; сначала альбомы, потом EP и синглы, по дате.
    let mut seen = std::collections::HashSet::new();
    albums.retain(|a| seen.insert(normalize(&s(a, "title"))));
    let rank = |a: &Value| match s(a, "record_type").as_str() {
        "album" => 0,
        "ep" => 1,
        "compile" => 3,
        _ => 2,
    };
    albums.sort_by(|a, b| rank(a).cmp(&rank(b)).then(s(a, "release_date").cmp(&s(b, "release_date"))));
    log(format!("Дискография «{name}»: альбомов и синглов — {}", albums.len()));
    let mut tracks = Vec::new();
    let mut have = std::collections::HashSet::new();
    for a in &albums {
        let Ok(full) = get_json(&format!("{api}/album/{}", a["id"]), &[]) else { continue };
        let year = s(&full, "release_date").chars().take(4).collect::<String>();
        let folder = if year.is_empty() { s(&full, "title") } else { format!("{year} - {}", s(&full, "title")) };
        for mut t in deezer_album_tracks(&full) {
            // Трек уже есть в другом альбоме (сингл → альбом) — второй раз не качаем.
            if !have.insert(format!("{}|{}", normalize(&t.artist), normalize(&t.title))) {
                continue;
            }
            t.album = folder.clone();
            tracks.push(t);
        }
        std::thread::sleep(std::time::Duration::from_millis(120)); // Deezer: не больше 50 запросов за 5 с
    }
    Ok(MusicList { name, service: "Дискография".into(), tracks, note: None, by_album: true, single: false })
}

/// Название для очереди вместо «vgdlist:C:\…»: «плейлист · VK · 25 треков».
pub fn list_title(url: &str) -> Option<String> {
    let l = read_list(Path::new(url.strip_prefix("vgdlist:")?)).ok()?;
    Some(if l.single { l.tracks.first()?.display() } else { format!("{} · {} · треков: {}", l.name, l.service, l.tracks.len()) })
}

// ---------- сравнение найденного трека с нужным ----------

/// Нижний регистр, без скобок «(Official Video)», «[Lyrics]», «feat.» и знаков.
pub fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.to_lowercase().replace('ё', "е").chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    let words: Vec<&str> = out.split_whitespace().take_while(|w| !matches!(*w, "feat" | "ft" | "featuring")).collect();
    words.join(" ")
}

/// Подходит ли найденное: длительность (если известна) и слова названия.
pub fn matches(want: &Track, cand_title: &str, cand_artist: &str, cand_duration: Option<f64>) -> bool {
    if let (Some(a), Some(b)) = (want.duration, cand_duration) {
        if (a - b).abs() > (a * 0.05).max(8.0) {
            return false;
        }
    }
    let hay = normalize(&format!("{cand_artist} {cand_title}"));
    let title = normalize(&want.title);
    let words: Vec<&str> = title.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    let hit = words.iter().filter(|w| hay.split_whitespace().any(|h| h == **w)).count();
    // Каверы, концертные и ускоренные версии — только если их просили.
    let want_all = normalize(&format!("{} {}", want.title, want.artist));
    for bad in ["cover", "live", "karaoke", "sped", "slowed", "nightcore", "reverb", "8d", "instrumental", "remix"] {
        let raw = cand_title.to_lowercase();
        let in_cand = raw.split(|c: char| !c.is_alphanumeric()).any(|w| w == bad);
        let in_want = format!("{} {}", want.title, want.artist).to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| w == bad);
        let _ = &want_all;
        if in_cand && !in_want {
            return false;
        }
    }
    hit * 3 >= words.len() * 2
}

/// Папка для музыки: <папка загрузок>\Музыка.
pub fn music_root(s: &Settings) -> PathBuf {
    PathBuf::from(&s.out_dir).join("Музыка")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_list_with_streams() {
        let path = std::env::temp_dir().join(format!("vgd-test-vk-{}.json", std::process::id()));
        let body = json!({ "name": "Мой плейлист", "service": "VK", "tracks": [
            { "id": "-2001_123", "artist": "A", "title": "B", "duration": 200, "url": "https://cs1.vkuseraudio.net/s/v1/ac/x/index.m3u8?siren=1", "cover": "https://sun9.userapi.com/c.jpg" },
            { "id": "-2001_124", "artist": "C", "title": "D", "url": "" }
        ]});
        std::fs::write(&path, body.to_string()).unwrap();
        let l = read_list(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(l.tracks.len(), 2);
        assert_eq!(l.tracks[0].key, "vk:-2001_123");
        assert_eq!(l.tracks[0].source.as_deref(), Some("https://cs1.vkuseraudio.net/s/v1/ac/x/index.m3u8?siren=1"));
        assert_eq!(l.tracks[0].cover.as_deref(), Some("https://sun9.userapi.com/c.jpg"));
        assert_eq!(l.tracks[1].source, None); // нет ссылки — программа ищет замену
    }

    #[test]
    fn spotify_page_tracks() {
        let l = tracks_from_page(
            r#"{"name":"All Out 80s","total":3,"tracks":[{"id":"a1","title":"Africa","artist":"TOTO","album":"Toto IV","duration":295},
               {"id":"b2","title":"Holiday","artist":"Madonna","duration":null},{"id":"c3","title":""}]}"#,
        )
        .unwrap();
        assert_eq!(l.name, "All Out 80s");
        assert_eq!(l.tracks.len(), 2);
        assert_eq!(l.tracks[0].key, "spotify:a1");
        assert_eq!(l.tracks[0].duration, Some(295.0));
        assert!(l.note.is_some(), "собрано меньше, чем в заголовке, — сказать об этом");
        assert!(tracks_from_page(r#"{"error":"нет списка"}"#).is_err());
    }

    #[test]
    fn vk_page_titles() {
        assert_eq!(page_title_track("ВКонтакте | ВКонтакте"), None);
        assert_eq!(page_title_track("Steve Lacy &#8212; Dark Red | VK"), Some(("Steve Lacy".into(), "Dark Red".into())));
        assert_eq!(
            meta_content(r#"<meta property="og:title" content="A — B" />"#, "og:title").as_deref(),
            Some("A — B")
        );
    }

    #[test]
    fn spotify_links() {
        assert_eq!(
            spotify_ref("https://open.spotify.com/playlist/5HPP8szpJyUcaYnDeEbHz3?si=4ca"),
            Some(("playlist".into(), "5HPP8szpJyUcaYnDeEbHz3".into()))
        );
        assert_eq!(
            spotify_ref("https://open.spotify.com/intl-ru/album/abc"),
            Some(("album".into(), "abc".into()))
        );
    }

    #[test]
    fn music_links() {
        assert!(is_music_url("https://vk.ru/music/playlist/429566295_18"));
        assert!(is_music_url("https://music.yandex.ru/artist/357420?utm_source=web"));
        assert!(!is_music_url("https://vk.com/video-77521_162222515"));
        assert!(!is_music_url("https://www.youtube.com/watch?v=x"));
    }

    #[test]
    fn search_lines() {
        for q in ["The Weeknd — Blinding Lights", "Кино - Группа крови", "numb", "Mr. Brightside", "AC/DC Thunderstruck", "Земфира искала"] {
            assert!(is_search(q), "{q}");
            assert!(is_music_url(q), "{q}");
        }
        for l in [
            "https://youtu.be/x",
            "youtu.be/dQw4w9WgXcQ",
            "www.youtube.com/watch?v=x",
            "vk.com/audio-1_2",
            "example.com",
            "vgdlist:C:\\a.json",
            "",
            "  ",
            "---",
        ] {
            assert!(!is_search(l), "{l}");
        }
    }

    #[test]
    fn matching() {
        let t = Track { artist: "Radiohead".into(), title: "Bullet Proof ... I Wish I Was".into(), duration: Some(208.7), ..Default::default() };
        assert!(matches(&t, "Bullet Proof ... I Wish I Was", "Radiohead", Some(209.0)));
        assert!(!matches(&t, "Bullet Proof ... I Wish I Was (Live)", "Radiohead", Some(209.0)));
        assert!(!matches(&t, "Bullet Proof ... I Wish I Was", "Radiohead", Some(260.0)));
        assert!(!matches(&t, "Creep", "Radiohead", Some(208.0)));
        assert_eq!(normalize("Song (Official Video) feat. X"), "song");
    }
}

#[cfg(test)]
mod live_spotify {
    /// `cargo test -p vgd-core --lib live_spotify -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn track_page() {
        let r = super::req("https://open.spotify.com/embed/track/3EaJDYHA0KnX88JvDhL9oa").send();
        match &r {
            Ok(x) => println!("status {} len {}", x.status, x.body.len()),
            Err(e) => println!("err {e}"),
        }
        assert_eq!(super::spotify_oembed_title("3EaJDYHA0KnX88JvDhL9oa").as_deref(), Some("Dark Red"));
        let l = super::spotify_embed("track", "3EaJDYHA0KnX88JvDhL9oa");
        println!("{:?}", l.as_ref().map(|l| (&l.name, l.tracks.len())));
        assert!(l.is_ok());
    }
}

#[cfg(test)]
mod search_live {
    /// `cargo test -p vgd-core search_live -- --ignored --nocapture` — поиск трека по названию (нужна сеть).
    #[test]
    #[ignore]
    fn find() {
        for q in ["Rick Ashley - Never Gonna Give You Up", "Кино Группа крови", "daft punk get lucky"] {
            let l = super::search_track(q, &|m| println!("{m}"));
            println!("  → {} | альбом {} | {:?}", l.tracks[0].display(), l.tracks[0].album, l.tracks[0].duration);
        }
    }
}
