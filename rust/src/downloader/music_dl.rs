//! Скачивание музыки: список треков (music.rs) → для каждого трека звук с самого сервиса,
//! а если его там нет или он заблокирован — с YouTube Music, SoundCloud или YouTube.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};


use super::{fail, first_line, rand_u64, str_of, DlError, Downloader, Progress, R};
use crate::music::{self, MusicList, Track};
use crate::net::is_network_error;
use crate::sort::safe_name;

const ARCHIVE: &str = ".music_archive.txt";

/// Где искать замену, по порядку: YouTube Music (официальные аудио), SoundCloud, обычный YouTube.
const SEARCH: [(&str, &str); 4] =
    [("ytmusic", "YouTube Music"), ("soundcloud", "SoundCloud"), ("audius", "Audius"), ("youtube", "YouTube")];

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Временный сбой сервера: 5xx или истёк срок ожидания ответа.
fn is_temporary_http(m: &str) -> bool {
    ["HTTP Error 500", "HTTP Error 502", "HTTP Error 503", "HTTP Error 504", "Gateway Time-out", "Read timed out", "timed out"]
        .iter()
        .any(|s| m.contains(s))
}

/// «исполнитель|название» для поиска уже скачанного (только первый исполнитель).
fn track_key(artist: &str, title: &str) -> String {
    let a = artist.split([',', '，', '&']).next().unwrap_or("").trim();
    format!("{}|{}", music::normalize(a), music::normalize(title))
}

/// Все треки в папке «Музыка» по именам файлов «01 - Исполнитель - Название.ext».
fn existing_tracks(root: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !["opus", "m4a", "mp3", "flac", "ogg", "webm", "wav"].contains(&ext.as_str()) {
                continue;
            }
            let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
            let mut parts: Vec<&str> = stem.split(" - ").collect();
            if parts.len() >= 3 && parts[0].chars().all(|c| c.is_ascii_digit()) {
                parts.remove(0);
            }
            if parts.len() >= 2 {
                out.insert(track_key(parts[0], &parts[1..].join(" - ")));
            }
        }
    }
    out
}

impl Downloader {
    /// Копия загрузчика для параллельной работы: те же настройки, отмена и вывод.
    fn fork(&self) -> Downloader {
        Downloader {
            s: self.s.clone(),
            on_progress: self.on_progress.clone(),
            log: self.log.clone(),
            cancel: self.cancel.clone(),
            p: Progress { notice: self.p.notice.clone(), ..Default::default() },
            errors: 0,
            results: Vec::new(),
            use_cookies: self.use_cookies,
            cookies_broken: self.cookies_broken,
            limit: None,
            opts: self.opts.clone(),
            tiktok: false,
        }
    }

    pub(super) fn run_music(&mut self, url: &str) -> R<Vec<String>> {
        self.p.stage = "info".into();
        self.p.message = "Получаю список треков…".into();
        self.emit();
        let log = self.log.clone();
        let mut tries = 0;
        let mut list = loop {
            let r = match music::resolve(url, &self.s, &|m| log(m)) {
                Some(Ok(l)) => Ok(l),
                Some(Err(e)) => Err(DlError::Failed(e)),
                None => self.music_list_ytdlp(url),
            };
            match r {
                // Нет связи при получении списка — ждём сеть и пробуем ещё раз.
                Err(DlError::Failed(m)) if is_network_error(&m) && tries < 20 => {
                    tries += 1;
                    self.wait_online()?;
                }
                other => break other?,
            }
        };
        if list.tracks.is_empty() {
            return fail("В ссылке нет треков (плейлист пустой или закрыт).");
        }
        if let Some(n) = &list.note {
            self.log(format!("Внимание: {n}"));
            self.p.notice = n.clone();
            self.emit();
        }
        let total = list.tracks.len();
        if let Some(lim) = self.limit {
            list.tracks.truncate(lim);
        }
        let root = music::music_root(&self.s);
        let dir = if list.single {
            root.join(safe_name(list.tracks[0].artist.split(',').next().unwrap_or("").trim(), 80))
        } else {
            root.join(safe_name(&list.name, 80))
        };
        std::fs::create_dir_all(&dir).map_err(|e| DlError::Failed(format!("Не удалось создать папку: {e}")))?;
        let archive_path = root.join(ARCHIVE);
        let archive: std::collections::HashSet<String> = std::fs::read_to_string(&archive_path)
            .map(|t| t.lines().map(|l| l.trim().to_string()).collect())
            .unwrap_or_default();
        if !list.single {
            self.log(format!(
                "{} «{}»: {total} трек(ов){}",
                list.service,
                list.name,
                if list.tracks.len() < total { format!(", качаю первые {}", list.tracks.len()) } else { String::new() }
            ));
        }

        // Первая проверка подписки на плейлист: запоминаем всё, что уже есть.
        if self.opts.seed_only {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&archive_path) {
                for t in &list.tracks {
                    let _ = writeln!(f, "{}", t.key);
                }
            }
            let msg = format!("Подписка «{}»: запомнил {} трек(ов) — дальше буду качать только новые.", list.name, list.tracks.len());
            self.log(msg.clone());
            self.p = Progress { stage: "done".into(), percent: 100.0, message: msg, title: list.name.clone(), ..Default::default() };
            self.emit();
            return Ok(Vec::new());
        }
        let n = list.tracks.len();
        let width = n.to_string().len().max(2);
        // Уже скачанное с любого сервиса (по «исполнитель — название» в именах файлов) — не качаем снова.
        let have = existing_tracks(&root);
        let mut skipped = 0;
        let mut todo = std::collections::VecDeque::new();
        let mut album_pos: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (i, t) in list.tracks.iter().enumerate() {
            // Номер: по порядку в списке, а в дискографии — по порядку внутри альбома.
            let num = if list.by_album {
                let c = album_pos.entry(t.album.clone()).or_insert(0);
                *c += 1;
                *c
            } else {
                i + 1
            };
            if archive.contains(&t.key) || have.contains(&track_key(&t.artist, &t.title)) {
                skipped += 1;
                continue;
            }
            todo.push_back((i, num, t.clone()));
        }
        if skipped > 0 && !todo.is_empty() {
            self.log(format!("Уже скачано раньше: {skipped} — пропускаю."));
        }
        let total = todo.len();
        let queue = std::sync::Arc::new(std::sync::Mutex::new(todo));
        let started = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel::<(Track, R<PathBuf>)>();
        // Несколько треков сразу: поиск и скачивание идут параллельно (быстрее в 2–3 раза).
        let workers = if list.single { 1 } else { 4 }.min(total.max(1));
        let mut handles = Vec::new();
        for _ in 0..workers {
            let mut w = self.fork();
            let (queue, started, tx) = (queue.clone(), started.clone(), tx.clone());
            let (dir, single, by_album) = (dir.clone(), list.single, list.by_album);
            handles.push(std::thread::spawn(move || loop {
                let Some((_i, num, t)) = queue.lock().unwrap().pop_front() else { break };
                if w.check_cancel().is_err() {
                    break;
                }
                if let Err(e) = w.wait_proxy() {
                    let _ = tx.send((t, Err(e)));
                    break;
                }
                let k = started.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                w.p = Progress {
                    notice: w.p.notice.clone(),
                    stage: "info".into(),
                    title: t.display(),
                    item: if single { String::new() } else { format!("{k}/{total}") },
                    message: format!("Ищу: {}", t.display()),
                    ..Default::default()
                };
                w.emit();
                let tdir = if by_album { dir.join(safe_name(&t.album, 100)) } else { dir.clone() };
                let _ = std::fs::create_dir_all(&tdir);
                let prefix = (!single).then(|| format!("{:0width$}", num, width = if by_album { 2 } else { width }));
                // Обрыв связи (переключили VPN) — ждём сеть и повторяем этот же трек, а не пропускаем его.
                let mut net_retries = 0;
                let result = loop {
                    match w.music_track(&t, num, &tdir, prefix.as_deref()) {
                        Err(DlError::Failed(m)) if is_network_error(&m) && net_retries < 30 => {
                            net_retries += 1;
                            w.log(format!("Связь оборвалась ({}) — жду и продолжу с этого трека.", first_line(&m)));
                            if let Err(e) = w.wait_online() {
                                break Err(e);
                            }
                        }
                        other => break other,
                    }
                };
                let cancelled = matches!(result, Err(DlError::Cancelled(_)));
                let _ = tx.send((t, result));
                if cancelled {
                    break;
                }
            }));
        }
        drop(tx);
        let mut m3u: Vec<String> = Vec::new();
        let mut not_found: Vec<String> = Vec::new();
        let mut cancelled: Option<DlError> = None;
        for (t, result) in rx {
            match result {
                Ok(path) => {
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&archive_path) {
                        let _ = writeln!(f, "{}", t.key);
                    }
                    if let Ok(rel) = path.strip_prefix(&dir) {
                        m3u.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                    self.results.push(path.to_string_lossy().into_owned());
                }
                Err(DlError::Cancelled(m)) => cancelled = Some(DlError::Cancelled(m)),
                Err(DlError::Failed(m)) => {
                    self.errors += 1;
                    not_found.push(t.display());
                    self.log(format!("Ошибка: не скачался «{}»: {}", t.display(), first_line(&m)));
                }
            }
        }
        for h in handles {
            let _ = h.join();
        }
        if let Some(c) = cancelled {
            return Err(c);
        }
        self.check_cancel()?;
        if !list.single && !m3u.is_empty() {
            // Плейлист для плеера: дописываем новые треки к уже скачанным раньше.
            let p = dir.join(format!("{}.m3u8", safe_name(&list.name, 80)));
            let mut lines: Vec<String> = std::fs::read_to_string(&p)
                .map(|t| t.lines().map(String::from).filter(|l| !l.is_empty() && l != "#EXTM3U").collect())
                .unwrap_or_default();
            for f in m3u {
                if !lines.contains(&f) {
                    lines.push(f);
                }
            }
            lines.sort();
            let _ = std::fs::write(&p, format!("#EXTM3U\n{}\n", lines.join("\n")));
        }
        super::remove_empty_dirs(&root.join(".parts"));
        if self.results.is_empty() && self.errors > 0 {
            return fail(format!("Ни один трек не скачался: {}", not_found.join("; ")));
        }
        let mut msg = if self.results.is_empty() {
            "Новых треков нет — всё уже скачано раньше.".to_string()
        } else {
            format!("Готово: {} трек(ов) в {}", self.results.len(), dir.display())
        };
        if skipped > 0 && !self.results.is_empty() {
            msg += &format!(", скачано раньше: {skipped}");
        }
        if !not_found.is_empty() {
            msg += &format!(", не найдено: {}", not_found.len());
        }
        self.log(msg.clone());
        self.p = Progress {
            stage: "done".into(),
            percent: 100.0,
            files: self.results.clone(),
            message: msg,
            title: list.name.clone(),
            notice: self.p.notice.clone(),
            ..Default::default()
        };
        self.emit();
        Ok(self.results.clone())
    }

    /// SoundCloud и YouTube Music: список разворачивает yt-dlp, звук берётся прямо оттуда.
    fn music_list_ytdlp(&mut self, url: &str) -> R<MusicList> {
        let args = vec!["-J".to_string(), "--flat-playlist".to_string()];
        let info = self.json_with_login_retry(&args, url)?;
        let kind = info.get("_type").and_then(Value::as_str).unwrap_or("video");
        let service = if url.contains("soundcloud") { "SoundCloud" } else { "YouTube Music" };
        let to_track = |e: &Value| -> Option<Track> {
            let src = str_of(e, &["url", "webpage_url"]);
            let id = str_of(e, &["id"]);
            if src.is_empty() && id.is_empty() {
                return None;
            }
            let ie = str_of(e, &["ie_key", "extractor_key"]).to_lowercase();
            Some(Track {
                artist: str_of(e, &["artist", "creator", "uploader", "channel"]).trim_end_matches(" - Topic").to_string(),
                title: str_of(e, &["track", "title"]),
                album: str_of(e, &["album"]),
                duration: e.get("duration").and_then(Value::as_f64),
                isrc: None,
                cover: None,
                source: Some(if src.is_empty() { format!("https://music.youtube.com/watch?v={id}") } else { src }),
                key: format!("{}:{}", if ie.is_empty() { service.to_lowercase().replace(' ', "") } else { ie }, if id.is_empty() { str_of(e, &["url"]) } else { id }),
            })
        };
        if kind == "playlist" || kind == "multi_video" {
            let tracks = info.get("entries").and_then(Value::as_array).into_iter().flatten().filter_map(to_track).collect();
            Ok(MusicList { name: str_of(&info, &["title", "id"]), service: service.into(), tracks, note: None, by_album: false, single: false })
        } else {
            let mut t = to_track(&info).ok_or_else(|| DlError::Failed("не понял ссылку".into()))?;
            t.source = Some(str_of(&info, &["webpage_url"]).if_empty_else(url));
            Ok(MusicList { name: t.display(), service: service.into(), tracks: vec![t], note: None, by_album: false, single: true })
        }
    }

    /// Один трек: сначала прямой источник, потом поиск замены.
    fn music_track(&mut self, t: &Track, num: usize, dir: &Path, name: Option<&str>) -> R<PathBuf> {
        let mut want = t.clone();
        if let Some(src) = t.source.clone() {
            // Сервер ответил 5xx / не дождались ответа (у VK бывает 504) — повторяем, а не ищем замену сразу.
            let mut info = self.music_info(&src);
            for wait in [3, 8] {
                match &info {
                    Err(DlError::Failed(m)) if is_temporary_http(m) => {
                        self.log(format!("«{}»: сервер не ответил ({}) — повторю через {wait} с.", want.display(), first_line(m)));
                        self.sleep(std::time::Duration::from_secs(wait))?;
                        info = self.music_info(&src);
                    }
                    _ => break,
                }
            }
            match info {
                Ok(info) => {
                    if want.title.is_empty() {
                        want.title = str_of(&info, &["track", "title"]);
                    }
                    if want.artist.is_empty() {
                        want.artist = str_of(&info, &["artist", "creator", "uploader"]).trim_end_matches(" - Topic").to_string();
                    }
                    if want.duration.is_none() {
                        want.duration = info.get("duration").and_then(Value::as_f64);
                    }
                    match self.music_download(info, &want, num, dir, name) {
                        Ok(p) => {
                            if t.key.starts_with("vk:") {
                                self.log(format!("«{}» — взял из VK.", want.display()));
                            }
                            return Ok(p);
                        }
                        Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                        // Обрыв связи — не повод искать замену: вернёмся к этому же треку, когда сеть появится.
                        Err(DlError::Failed(m)) if is_network_error(&m) => return fail(m),
                        Err(DlError::Failed(m)) => self.log(format!("«{}» не скачался с сервиса ({}) — ищу замену.", want.display(), first_line(&m))),
                    }
                }
                Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                Err(DlError::Failed(m)) if is_network_error(&m) => return fail(m),
                Err(DlError::Failed(m)) => {
                    self.log(format!("«{}» недоступен на сервисе ({}) — ищу замену.", want.display(), first_line(&m)));
                    if want.title.is_empty() {
                        // Название из ссылки: soundcloud.com/автор/название-трека
                        want.title = src.trim_end_matches('/').rsplit('/').next().unwrap_or("").replace('-', " ");
                    }
                }
            }
        }
        if want.title.is_empty() {
            return fail("нет названия для поиска");
        }
        // Нет ISRC (Spotify без ключа, VK, Яндекс) — узнаём его у Deezer: так находится точная запись.
        if want.isrc.is_none() && !want.artist.is_empty() {
            self.p.message = format!("Уточняю трек: {}", want.display());
            self.emit();
            want.isrc = match want.key.strip_prefix("deezer:") {
                Some(id) => music::deezer_track_isrc(id), // трек из каталога Deezer — ISRC одним запросом
                None => music::deezer_isrc(&want),
            };
        }
        let mut queries: Vec<(&str, &str, String)> = Vec::new();
        let base = format!("{} {}", want.artist.split(',').next().unwrap_or("").trim(), want.title).trim().to_string();
        if let Some(isrc) = &want.isrc {
            queries.push(("ytmusic", "YouTube Music", format!("\"{isrc}\"")));
        }
        let artist1 = want.artist.split(',').next().unwrap_or("").trim().to_string();
        for (kind, label) in SEARCH {
            queries.push((kind, label, base.clone()));
            // Редкие треки находятся точнее, если исполнитель в кавычках.
            if !artist1.is_empty() && kind != "soundcloud" && kind != "audius" {
                queries.push((kind, label, format!("\"{artist1}\" {}", want.title)));
            }
        }
        // Для проверки отдельного источника: VGD_ONLY_SOURCE=audius (или ytmusic, soundcloud, youtube).
        if let Ok(only) = std::env::var("VGD_ONLY_SOURCE") {
            queries.retain(|(k, _, _)| *k == only.as_str());
        }
        let mut last_err = String::from("ничего похожего не нашлось");
        for (kind, label, q) in queries {
            self.check_cancel()?;
            self.p.message = format!("Ищу на {label}: {}", want.display());
            self.emit();
            let cands = match self.music_search(kind, &q) {
                Ok(c) => c,
                Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                Err(DlError::Failed(m)) if is_network_error(&m) => return fail(m),
                Err(DlError::Failed(m)) => {
                    last_err = m;
                    continue;
                }
            };
            for (url, title, uploader, dur) in cands.into_iter().take(4) {
                // Дешёвая проверка по выдаче (название, автор, длительность), полная — по сведениям трека.
                if !title.is_empty()
                    && !music::matches(&want, &title, &uploader, dur)
                    && !music::matches(&want, &format!("{} {}", want.artist, title), &uploader, dur)
                {
                    continue;
                }
                let info = match self.music_info(&url) {
                    Ok(i) => i,
                    Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                    Err(DlError::Failed(m)) if is_network_error(&m) => return fail(m),
                    Err(DlError::Failed(m)) => {
                        last_err = m;
                        continue;
                    }
                };
                let ct = str_of(&info, &["track", "title"]);
                let ca = str_of(&info, &["artist", "creator", "uploader", "channel"]);
                let cd = info.get("duration").and_then(Value::as_f64);
                // У Audius ссылка ведёт прямо на mp3 — сведения уже проверены по ответу поиска.
                if kind != "audius" && !music::matches(&want, &ct, &ca, cd) {
                    continue;
                }
                match self.music_download(info, &want, num, dir, name) {
                    Ok(p) => {
                        if t.source.is_some() || !label.is_empty() {
                            self.log(format!("«{}» — взял с {label}.", want.display()));
                        }
                        return Ok(p);
                    }
                    Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                    Err(DlError::Failed(m)) if is_network_error(&m) => return fail(m),
                    Err(DlError::Failed(m)) => last_err = m,
                }
            }
        }
        fail(last_err)
    }

    /// Поиск: (ссылка, название) лучших совпадений.
    fn music_search(&mut self, kind: &str, q: &str) -> R<Vec<(String, String, String, Option<f64>)>> {
        if kind == "audius" {
            return music::audius_search(q).map_err(DlError::Failed);
        }
        let target = match kind {
            "ytmusic" => format!("https://music.youtube.com/search?q={}#songs", urlencode(q)),
            "soundcloud" => format!("scsearch4:{q}"),
            _ => format!("ytsearch4:{q}"),
        };
        let mut args = vec!["-J".to_string(), "--flat-playlist".to_string(), "--playlist-end".to_string(), "4".to_string()];
        args.push("--".into());
        args.push(target);
        let r = self.ytdlp(&args, |_, _| {})?;
        if !r.ok {
            return fail(r.errors.last().cloned().unwrap_or_else(|| "поиск не удался".into()));
        }
        let v = super::parse_json(&r.stdout)?;
        Ok(v.get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let url = str_of(e, &["url", "webpage_url"]);
                (!url.is_empty()).then(|| {
                    (url, str_of(e, &["title"]), str_of(e, &["uploader", "channel"]), e.get("duration").and_then(Value::as_f64))
                })
            })
            .collect())
    }

    /// Сведения о треке (`yt-dlp -j`), только звук.
    fn music_info(&mut self, url: &str) -> R<Value> {
        let args = vec!["-j".to_string(), "--no-playlist".to_string(), "-f".to_string(), "bestaudio/best".to_string()];
        self.json_with_login_retry(&args, url)
    }

    /// Скачать звук по сведениям, вписав правильные теги и обложку.
    fn music_download(&mut self, mut info: Value, t: &Track, num: usize, dir: &Path, prefix: Option<&str>) -> R<PathBuf> {
        // Имя файла: «01 - Исполнитель - Название» (в плейлисте) или «Исполнитель - Название».
        let base = t.display().replace(" — ", " - ");
        let name = safe_name(&match prefix {
            Some(p) => format!("{p} - {base}"),
            None => base,
        }.replace(['/', '\\'], "_"), 150);
        if !t.title.is_empty() {
            info["title"] = json!(t.title);
            info["track"] = json!(t.title);
        }
        if !t.artist.is_empty() {
            info["artist"] = json!(t.artist);
            info["artists"] = json!(t.artist.split(", ").collect::<Vec<_>>());
            info["creator"] = json!(t.artist);
        }
        if !t.album.is_empty() {
            info["album"] = json!(t.album);
        }
        info["track_number"] = json!(num);
        // Текст песни (LRCLIB): yt-dlp сам впишет поле meta_lyrics в тег файла (--embed-metadata).
        let mut synced_lyrics = None;
        if self.s.lyrics {
            self.p.message = format!("Ищу текст: {}", t.display());
            self.emit();
            if let Some((synced, plain)) = music::lrclib(t) {
                let text = plain.unwrap_or_else(|| lrc_to_plain(synced.as_deref().unwrap_or("")));
                if !text.trim().is_empty() {
                    info["meta_lyrics"] = json!(text);
                    self.log(format!("Текст песни найден{}", if synced.is_some() { " (с таймингом — .lrc рядом)" } else { "" }));
                }
                synced_lyrics = synced;
            }
        }
        if let Some(c) = &t.cover {
            // Квадратная обложка альбома из Spotify/Яндекса вместо кадра из видео.
            info["thumbnails"] = json!([{ "url": c, "id": "cover", "preference": 100 }]);
            info["thumbnail"] = json!(c);
        }
        let root = music::music_root(&self.s);
        let parts = root.join(".parts");
        let _ = std::fs::create_dir_all(&parts);
        let json_path = parts.join(format!("music-{}.json", rand_u64()));
        std::fs::write(&json_path, serde_json::to_vec(&info).unwrap_or_default())
            .map_err(|e| DlError::Failed(format!("временный файл: {e}")))?;
        let mut args: Vec<String> = vec![
            "--load-info-json".into(), json_path.to_string_lossy().into_owned(),
            "-f".into(), "bestaudio/best".into(), "-x".into(),
            "-P".into(), format!("home:{}", dir.display()),
            "-P".into(), format!("temp:{}", parts.display()),
            "-o".into(), format!("{}.%(ext)s", name.replace('%', "%%")),
            "--windows-filenames".into(), "--force-overwrites".into(),
            "--retries".into(), "15".into(), "--fragment-retries".into(), "30".into(),
            "--embed-metadata".into(), "--embed-thumbnail".into(), "--convert-thumbnails".into(), "jpg".into(),
            // Кадр из видео 16:9 → квадрат по центру, как обложка альбома.
            "--ppa".into(), r#"ThumbnailsConvertor+ffmpeg_o:-vf "crop=min(iw\,ih):min(iw\,ih)""#.into(),
            "--newline".into(), "--progress".into(),
            "--progress-template".into(),
            "download:VGDP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|none|%(info.format_note)s|%(info.format_id)s".into(),
            "--print".into(), "after_move:VGDF|%(filepath)s".into(),
        ];
        match self.s.music_format.as_str() {
            "mp3" => args.extend(["--audio-format", "mp3", "--audio-quality", "320K"].map(String::from)),
            "m4a" => args.extend(["--audio-format", "m4a", "--audio-quality", "0"].map(String::from)),
            "flac" => args.extend(["--audio-format", "flac"].map(String::from)),
            _ => {} // как есть: лучший звук без перекодирования (opus / m4a)
        }
        if self.use_cookies {
            args.extend(self.cookie_args());
        }
        self.p.stage = "audio".into();
        self.p.message = format!("Скачиваю: {}", t.display());
        self.emit();
        let mut final_path: Option<String> = None;
        let mut finished = 0;
        let r = self.ytdlp(&args, |me, line| {
            if let Some(rest) = line.strip_prefix("VGDF|") {
                final_path = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("VGDP|") {
                me.on_progress_line(rest, &Value::Null, &mut finished);
            }
        });
        let _ = std::fs::remove_file(&json_path);
        let r = r?;
        if !r.ok {
            return fail(r.errors.last().cloned().unwrap_or_else(|| "yt-dlp завершился с ошибкой".into()));
        }
        match final_path {
            Some(p) if Path::new(&p).exists() => {
                self.log(format!("Трек: {}", p));
                let path = PathBuf::from(p);
                if let Some(lrc) = &synced_lyrics {
                    // .lrc рядом — для плееров с бегущим текстом
                    let _ = std::fs::write(path.with_extension("lrc"), lrc);
                }
                Ok(path)
            }
            _ => fail(r.errors.last().cloned().unwrap_or_else(|| "файл не появился".into())),
        }
    }
}

/// Текст без меток времени из .lrc («[00:12.34] строка» → «строка»).
fn lrc_to_plain(lrc: &str) -> String {
    lrc.lines()
        .map(|l| if l.starts_with('[') { l.splitn(2, ']').nth(1).unwrap_or("").trim() } else { l.trim() })
        .collect::<Vec<_>>()
        .join("\n")
}

trait IfEmptyElse {
    fn if_empty_else(self, o: &str) -> String;
}

impl IfEmptyElse for String {
    fn if_empty_else(self, o: &str) -> String {
        if self.is_empty() { o.to_string() } else { self }
    }
}
