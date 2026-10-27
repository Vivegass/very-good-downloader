//! Скачивание одной ссылки (видео, плейлист или канал) через yt-dlp.exe + доработка через ffmpeg.
//!
//! Порядок для каждого видео:
//! 1. `yt-dlp -j` — сведения о видео (без куки; с куки браузера — только если без них нельзя).
//! 2. Тема и папка канала считаются здесь и вписываются в сведения (`sort_topic`, `sort_channel`).
//! 3. `yt-dlp --load-info-json` — скачивание по шаблону `%(sort_topic)s/%(sort_channel)s/…`.
//! 4. Пересчёт fps, субтитры отдельным проходом, копия для монтажа (ffmpeg).
//! 5. Запись в `.ytdl_archive.txt` — только после всей обработки (иначе при повторе видео перекачается).

use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::appinfo::{command, data_dir, tool, vtuple};
use crate::media::{fmt_bytes, fmt_eta, probe};
use crate::net::{current_proxy, is_network_error, is_proxy_error};
use crate::settings::{edit_name, JobOpts, Settings};
use crate::sort::Sorter;

mod mix;
mod music_dl;

pub const MIN_FREE_GB: f64 = 15.0;
const VIDEO_TMPL: &str = "%(title).150B [%(id)s].%(ext)s";
const ARCHIVE_NAME: &str = ".ytdl_archive.txt";

#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// "info", "video", "audio", "merge", "edit", "subs", "wait", "done"
    pub stage: String,
    pub title: String,
    pub percent: f32,
    pub speed: String,
    pub eta: String,
    /// "3/25" для плейлистов
    pub item: String,
    pub message: String,
    pub files: Vec<String>,
    /// важное для пользователя (например, «Spotify отдал только 100 треков») — показывается в очереди
    pub notice: String,
}

#[derive(Debug)]
pub enum DlError {
    Cancelled(String),
    Failed(String),
}

impl std::fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlError::Cancelled(m) if m.is_empty() => write!(f, "Отменено"),
            DlError::Cancelled(m) | DlError::Failed(m) => write!(f, "{m}"),
        }
    }
}

type R<T> = Result<T, DlError>;

fn fail<T>(msg: impl Into<String>) -> R<T> {
    Err(DlError::Failed(msg.into()))
}

/// Отмена из другого потока: флаг + принудительное завершение текущего yt-dlp/ffmpeg.
#[derive(Clone, Default)]
pub struct CancelHandle {
    flag: Arc<AtomicBool>,
    child: Arc<Mutex<Vec<u32>>>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        let pids = self.child.lock().unwrap().clone();
        for pid in pids {
            kill_tree(pid);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// yt-dlp.exe — это распаковщик + процесс Python + ffmpeg; гасим всё дерево.
fn kill_tree(pid: u32) {
    let _ = command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).output();
}

pub type ProgressFn = Arc<dyn Fn(&Progress) + Send + Sync>;
pub type LogFn = Arc<dyn Fn(String) + Send + Sync>;

/// Один элемент ссылки (видео в плейлисте или само видео).
#[derive(Clone, Debug)]
struct Entry {
    url: String,
    id: String,
    /// "youtube" — как в архиве yt-dlp
    extractor: String,
}

struct Listing {
    title: String,
    id: String,
    is_playlist: bool,
    entries: Vec<Entry>,
    /// для одиночного видео сведения уже получены
    single_info: Option<Value>,
}

/// Результат вызова yt-dlp.
struct Run {
    ok: bool,
    stdout: String,
    errors: Vec<String>,
}

pub struct Downloader {
    s: Settings,
    on_progress: ProgressFn,
    log: LogFn,
    cancel: CancelHandle,
    p: Progress,
    errors: usize,
    results: Vec<String>,
    /// куки понадобились хотя бы раз — дальше по этой ссылке сразу с ними
    use_cookies: bool,
    /// куки браузера не читаются — больше не пробуем
    cookies_broken: bool,
    /// не больше стольких треков/видео из плейлиста (консоль: --limit)
    pub limit: Option<usize>,
    /// фрагмент, только звук, подписка
    pub opts: JobOpts,
    /// ссылка TikTok — предпочитаем H.264 (HEVC многие программы не открывают)
    tiktok: bool,
    /// ссылка, которую просили скачать (для «_откуда скачано.txt»)
    asked: String,
}

impl Downloader {
    pub fn new(settings: Settings, on_progress: ProgressFn, log: LogFn) -> Self {
        Self {
            s: settings,
            on_progress,
            log,
            cancel: CancelHandle::default(),
            p: Progress::default(),
            errors: 0,
            results: Vec::new(),
            use_cookies: false,
            cookies_broken: false,
            limit: None,
            opts: JobOpts::default(),
            tiktok: false,
            asked: String::new(),
        }
    }

    pub fn cancel_handle(&self) -> CancelHandle {
        self.cancel.clone()
    }

    fn log(&self, m: impl Into<String>) {
        (self.log)(m.into());
    }

    fn emit(&self) {
        (self.on_progress)(&self.p);
    }

    fn check_cancel(&self) -> R<()> {
        if self.cancel.is_cancelled() {
            Err(DlError::Cancelled(String::new()))
        } else {
            Ok(())
        }
    }

    fn out(&self) -> PathBuf {
        PathBuf::from(&self.s.out_dir)
    }

    // ---------- основной запуск ----------

    /// Возвращает список итоговых файлов (оригиналы и копии для монтажа).
    pub fn run(&mut self, url: &str) -> R<Vec<String>> {
        self.p = Progress { stage: "info".into(), title: url.into(), message: "Получаю информацию…".into(), ..Default::default() };
        self.emit();
        self.results.clear();
        self.errors = 0;
        self.use_cookies = false;
        self.cookies_broken = false;
        std::fs::create_dir_all(self.out()).map_err(|e| DlError::Failed(format!("Не удалось создать папку: {e}")))?;
        check_disk(&self.s.out_dir)?;
        self.wait_proxy()?;
        self.tiktok = url.contains("tiktok.com");
        self.asked = url.to_string();
        // «Микс → треки» — это видео YouTube, даже если ссылка с music.youtube.com.
        if crate::music::is_music_url(url) && !self.opts.split_tracks {
            return self.run_music(url);
        }

        let listing = self.list(url)?;
        let n = listing.entries.len();
        let mut archive = read_archive(&self.out());
        let todo: Vec<(usize, Entry)> = listing
            .entries
            .iter()
            .cloned()
            .enumerate()
            // Фрагмент или «только звук» — это другой файл: качаем, даже если видео уже скачано.
            .filter(|(_, e)| self.opts.is_variant() || !archive.contains(&archive_key(e)))
            .collect();
        // Первая проверка подписки: запоминаем всё, что уже есть, — дальше качаются только новые.
        if self.opts.seed_only {
            for (_, e) in &todo {
                append_archive(&self.out(), &archive_key(e));
            }
            let msg = format!("Подписка «{}»: запомнил {} видео — дальше буду качать только новые.", listing.title, todo.len());
            self.log(msg.clone());
            self.p = Progress { stage: "done".into(), percent: 100.0, message: msg, title: listing.title.clone(), ..Default::default() };
            self.emit();
            return Ok(Vec::new());
        }
        if listing.is_playlist {
            self.log(format!(
                "Плейлист «{}»: {n} видео{}",
                listing.title,
                if n > todo.len() { format!(" (скачано раньше: {})", n - todo.len()) } else { String::new() }
            ));
        }

        let mut retry_403: Vec<(usize, Entry)> = Vec::new();
        let mut last_download: Option<Instant> = None;
        for (i, e) in &todo {
            self.check_cancel()?;
            let single = if listing.is_playlist { None } else { listing.single_info.clone() };
            match self.video_with_pauses(&listing, *i, n, e, single, &mut last_download) {
                Ok(()) => {
                    archive.insert(archive_key(e));
                }
                Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                Err(DlError::Failed(m)) => {
                    if m.contains("HTTP Error 403") {
                        self.log(format!("YouTube временно отказал (403): {} — повторю в конце.", e.id));
                        retry_403.push((*i, e.clone()));
                    } else {
                        self.error(&e.id, &m);
                        if !listing.is_playlist {
                            return fail(m);
                        }
                    }
                }
            }
        }

        // Повтор видео, на которых YouTube ответил 403: обычно через минуту всё проходит.
        if !retry_403.is_empty() {
            self.p.stage = "wait".into();
            self.p.message = format!("Повтор после 403 ({} видео) через 30 с…", retry_403.len());
            self.emit();
            self.log(self.p.message.clone());
            self.sleep(Duration::from_secs(30))?;
            for (i, e) in &retry_403 {
                self.check_cancel()?;
                match self.video_with_pauses(&listing, *i, n, e, None, &mut last_download) {
                    Ok(()) => {}
                    Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                    Err(DlError::Failed(m)) => {
                        self.error(&e.id, &m);
                        if !listing.is_playlist {
                            return fail(m);
                        }
                    }
                }
            }
        }

        remove_empty_dirs(&self.out().join(".parts"));
        if self.results.is_empty() && self.errors > 0 {
            return fail("Ни одно видео не скачалось (подробности в журнале).");
        }
        let mut msg = if self.results.is_empty() {
            "Новых видео нет — всё уже скачано раньше.".to_string()
        } else {
            format!("Готово: {} файл(ов)", self.results.len())
        };
        if self.errors > 0 {
            msg += &format!(", ошибок: {} (см. журнал)", self.errors);
        }
        self.log(if n > 1 { format!("{msg} · видео в ссылке: {n}") } else { msg.clone() });
        self.p = Progress { stage: "done".into(), percent: 100.0, files: self.results.clone(), message: msg, title: self.p.title.clone(), item: String::new(), ..Default::default() };
        self.emit();
        Ok(self.results.clone())
    }

    fn error(&mut self, id: &str, msg: &str) {
        self.errors += 1;
        let first = msg.lines().next().unwrap_or("").chars().take(300).collect::<String>();
        self.log(format!("Ошибка: {id}: {first}"));
    }

    /// Пауза между видео, ожидание прокси и повтор, если прокси пропал посреди загрузки.
    fn video_with_pauses(
        &mut self,
        listing: &Listing,
        i: usize,
        n: usize,
        e: &Entry,
        single: Option<Value>,
        last: &mut Option<Instant>,
    ) -> R<()> {
        let mut net_retries = 0;
        loop {
            if let Some(t) = *last {
                // 2–6 с между видео, как sleep_interval в Python-версии: меньше шансов получить 429/403.
                let pause = Duration::from_millis(2000 + rand_u64() % 4000);
                if let Some(rest) = pause.checked_sub(t.elapsed()) {
                    self.sleep(rest)?;
                }
            }
            self.wait_proxy()?;
            self.p.item = if listing.is_playlist { format!("{}/{}", i + 1, n) } else { String::new() };
            let r = self.video(listing, i, n, e, single.clone());
            *last = Some(Instant::now());
            match r {
                Err(DlError::Failed(m)) if is_proxy_error(&m) && self.proxy_down() => {
                    self.log("Связь через VPN/прокси пропала — дождусь её и повторю это видео.");
                    continue;
                }
                // Переключили VPN / пропала сеть: ждём связь и продолжаем это же видео
                // (yt-dlp докачивает .part-файл с того места, где остановился).
                Err(DlError::Failed(m)) if is_network_error(&m) && net_retries < 30 => {
                    net_retries += 1;
                    self.log(format!("Связь оборвалась ({}) — жду и продолжу это видео.", first_line(&m)));
                    self.wait_online()?;
                    continue;
                }
                other => return other,
            }
        }
    }

    /// Прокси указан, но не отвечает.
    fn proxy_down(&self) -> bool {
        current_proxy().is_some_and(|p| !p.alive())
    }

    /// Если прокси/VPN не отвечает — понятное сообщение и пауза до его возвращения (с отменой).
    fn wait_proxy(&mut self) -> R<()> {
        let Some(proxy) = current_proxy() else { return Ok(()) };
        if proxy.alive() {
            return Ok(());
        }
        let msg = format!("{} — жду, пока он заработает (Отменить — остановить)…", proxy.describe());
        self.log(format!("Внимание: {msg}"));
        let saved = (self.p.stage.clone(), self.p.message.clone());
        self.p.stage = "wait".into();
        self.p.message = msg;
        self.p.speed.clear();
        self.p.eta.clear();
        self.emit();
        loop {
            self.sleep(Duration::from_secs(3))?;
            if current_proxy().map_or(true, |p| p.alive()) {
                break;
            }
        }
        self.log("VPN/прокси снова отвечает — продолжаю.");
        (self.p.stage, self.p.message) = saved;
        self.emit();
        Ok(())
    }

    /// Нет интернета (VPN переключается, сеть пропала) — ждём, пока появится, с отменой.
    fn wait_online(&mut self) -> R<()> {
        self.wait_proxy()?;
        if crate::net::online() {
            return self.sleep(Duration::from_secs(2));
        }
        let saved = (self.p.stage.clone(), self.p.message.clone());
        self.p.stage = "wait".into();
        self.p.message = "Нет связи (переключается VPN?) — жду и продолжу с того же места…".into();
        self.p.speed.clear();
        self.p.eta.clear();
        self.emit();
        self.log(format!("Внимание: {}", self.p.message));
        loop {
            self.sleep(Duration::from_secs(3))?;
            self.wait_proxy()?;
            if crate::net::online() {
                break;
            }
        }
        self.log("Связь вернулась — продолжаю.");
        (self.p.stage, self.p.message) = saved;
        self.emit();
        Ok(())
    }

    fn sleep(&self, d: Duration) -> R<()> {
        let end = Instant::now() + d;
        while Instant::now() < end {
            self.check_cancel()?;
            std::thread::sleep(Duration::from_millis(200).min(end - Instant::now()));
        }
        self.check_cancel()
    }

    // ---------- список видео ----------

    fn list(&mut self, url: &str) -> R<Listing> {
        let mut args: Vec<String> = vec!["-J".into(), "--flat-playlist".into()];
        // «Микс» YouTube (list=RD…) бесконечный — из такой ссылки берём только само видео.
        let mix = url.contains("list=RD") || url.contains("start_radio=");
        if !self.s.playlist || mix {
            args.push("--no-playlist".into());
        }
        args.extend(self.format_args());
        let info = self.json_with_login_retry(&args, url)?;
        let mut entries = Vec::new();
        let kind = info.get("_type").and_then(Value::as_str).unwrap_or("video");
        if kind == "playlist" || kind == "multi_video" {
            self.collect_entries(&info, &mut entries, 0)?;
            let title = str_of(&info, &["title", "id"]);
            return Ok(Listing { title, id: str_of(&info, &["id"]), is_playlist: true, entries, single_info: None });
        }
        let e = Entry {
            url: str_of(&info, &["webpage_url", "original_url"]).if_empty(url),
            id: str_of(&info, &["id"]),
            extractor: str_of(&info, &["extractor_key", "ie_key"]).to_lowercase(),
        };
        Ok(Listing { title: String::new(), id: String::new(), is_playlist: false, entries: vec![e], single_info: Some(info) })
    }

    fn collect_entries(&mut self, info: &Value, out: &mut Vec<Entry>, depth: usize) -> R<()> {
        let Some(list) = info.get("entries").and_then(Value::as_array) else { return Ok(()) };
        for e in list.iter().filter(|e| e.is_object()) {
            let ie = str_of(e, &["ie_key", "extractor_key"]);
            let url = str_of(e, &["url", "webpage_url"]);
            let nested = e.get("_type").and_then(Value::as_str) == Some("playlist")
                || ie.ends_with("Tab")
                || ie.contains("Playlist");
            if nested {
                // Канал целиком: вкладки «Видео», «Shorts», «Трансляции» — это вложенные плейлисты.
                if depth < 2 {
                    let sub = if e.get("entries").is_some() {
                        e.clone()
                    } else {
                        self.check_cancel()?;
                        let args = vec!["-J".to_string(), "--flat-playlist".to_string()];
                        match self.json_with_login_retry(&args, &url) {
                            Ok(v) => v,
                            Err(DlError::Failed(m)) => {
                                self.log(format!("Предупреждение: пропускаю {url}: {}", first_line(&m)));
                                continue;
                            }
                            Err(c) => return Err(c),
                        }
                    };
                    self.collect_entries(&sub, out, depth + 1)?;
                }
                continue;
            }
            let id = str_of(e, &["id"]);
            if id.is_empty() && url.is_empty() {
                continue;
            }
            let extractor = if ie.is_empty() { "youtube".into() } else { ie.to_lowercase() };
            let url = if url.is_empty() { format!("https://www.youtube.com/watch?v={id}") } else { url };
            if !out.iter().any(|x: &Entry| x.id == id && !id.is_empty()) {
                out.push(Entry { url, id, extractor });
            }
        }
        Ok(())
    }

    // ---------- одно видео ----------

    fn video(&mut self, listing: &Listing, i: usize, n: usize, e: &Entry, single: Option<Value>) -> R<()> {
        self.p.stage = "info".into();
        self.p.percent = 0.0;
        self.p.speed.clear();
        self.p.eta.clear();
        self.p.message = format!("Получаю сведения о видео {}…", e.id);
        self.emit();
        let mut info = match single {
            Some(v) => v,
            None => {
                let mut args = vec!["-j".to_string(), "--no-playlist".to_string()];
                args.extend(self.format_args());
                self.json_with_login_retry(&args, &e.url)?
            }
        };
        let title = str_of(&info, &["title"]);
        self.p.title = title.clone();
        // Подписка «не старше N дней»: старое пропускаем и запоминаем, чтобы не проверять снова.
        if self.opts.max_age_days > 0 {
            if let Some(age) = upload_age_days(&str_of(&info, &["upload_date"])) {
                if age > self.opts.max_age_days as i64 {
                    self.log(format!("Пропускаю (старше {} дн.): {title}", self.opts.max_age_days));
                    append_archive(&self.out(), &archive_key_info(&info, e));
                    return Ok(());
                }
            }
        }
        if self.opts.split_tracks {
            return self.mix_tracks(&info, e);
        }
        if listing.is_playlist {
            // Для сортировки (название плейлиста ×4) и шаблона без сортировки.
            info["playlist_title"] = Value::String(listing.title.clone());
            info["playlist"] = Value::String(listing.title.clone());
            info["playlist_id"] = Value::String(listing.id.clone());
            info["playlist_index"] = Value::from(i + 1);
            info["n_entries"] = Value::from(n);
        }
        let tmpl = if self.s.sort {
            let (topic, chan) = Sorter::new(self.out()).target_dir(&info);
            info["sort_topic"] = Value::String(topic);
            info["sort_channel"] = Value::String(chan);
            format!("%(sort_topic)s/%(sort_channel)s/{VIDEO_TMPL}")
        } else if listing.is_playlist && self.s.playlist {
            format!("%(playlist_title,playlist_id)s/%(playlist_index)03d - {VIDEO_TMPL}")
        } else {
            VIDEO_TMPL.to_string()
        };
        // Фрагмент — своё имя файла, чтобы не затереть полное видео.
        let tmpl = if self.opts.has_section() {
            let tag = format!(" (фрагмент {}–{})", self.opts.from.trim(), self.opts.to.trim()).replace(':', "-").replace(['/', '\\', '%'], "_");
            tmpl.replace(".%(ext)s", &format!("{tag}.%(ext)s"))
        } else {
            tmpl
        };

        let parts = self.out().join(".parts");
        crate::appinfo::hidden_dir(&parts);
        let json_path = parts.join(format!("{}.vgd.info.json", if e.id.is_empty() { "video" } else { &e.id }));
        std::fs::write(&json_path, serde_json::to_vec(&info).unwrap_or_default())
            .map_err(|err| DlError::Failed(format!("Не удалось записать временный файл: {err}")))?;
        let result = self.download(&json_path, &tmpl, &info);
        let path = match result {
            Ok(p) => p,
            Err(err) => {
                let _ = std::fs::remove_file(&json_path);
                return Err(err);
            }
        };
        // Видео уже скачано: ошибки доработки не отменяют его (как в Python-версии).
        match self.finish_video(&info, &json_path, &path) {
            Ok(()) => {}
            Err(DlError::Cancelled(m)) => {
                let _ = std::fs::remove_file(&json_path);
                return Err(DlError::Cancelled(m));
            }
            Err(DlError::Failed(m)) => {
                let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.errors += 1;
                self.log(format!("Ошибка: обработка {name}: {}", first_line(&m)));
            }
        }
        let _ = std::fs::remove_file(&json_path);
        if !self.opts.is_variant() {
            append_archive(&self.out(), &archive_key_info(&info, e));
        }
        Ok(())
    }

    /// Скачивает по готовым сведениям. Возвращает путь к итоговому файлу.
    fn download(&mut self, json_path: &Path, tmpl: &str, info: &Value) -> R<PathBuf> {
        let out = self.out();
        let mut args: Vec<String> = vec![
            "--load-info-json".into(), json_path.to_string_lossy().into_owned(),
            "--merge-output-format".into(), "mkv".into(),
            "-P".into(), format!("home:{}", out.display()),
            "-P".into(), format!("temp:{}", out.join(".parts").display()),
            "-o".into(), tmpl.into(),
            "--windows-filenames".into(), "--continue".into(),
            "--retries".into(), "20".into(), "--fragment-retries".into(), "50".into(),
            "--concurrent-fragments".into(), "4".into(),
            "--newline".into(), "--progress".into(),
            "--progress-template".into(),
            "download:VGDP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.vcodec)s|%(info.format_note)s|%(info.format_id)s".into(),
            "--print".into(), "after_move:VGDF|%(filepath)s".into(),
        ];
        args.extend(self.format_args());
        if self.opts.audio_only {
            args.push("-x".into());
            match self.s.music_format.as_str() {
                "mp3" => args.extend(["--audio-format", "mp3", "--audio-quality", "320K"].map(String::from)),
                "m4a" => args.extend(["--audio-format", "m4a", "--audio-quality", "0"].map(String::from)),
                "flac" => args.extend(["--audio-format", "flac"].map(String::from)),
                _ => {}
            }
            args.extend(["--embed-metadata".into(), "--embed-thumbnail".into()]);
        }
        if self.opts.has_section() {
            // Точный рез по времени (ffmpeg перекодирует края фрагмента).
            args.extend(["--download-sections".into(), self.opts.section(), "--force-keyframes-at-cuts".into()]);
        }
        if self.s.sponsorblock {
            args.extend(["--sponsorblock-remove".into(), "sponsor,selfpromo,interaction".into()]);
        }
        if self.s.split_chapters && !self.opts.has_section() {
            // Главы — отдельными файлами в папке «<видео> — главы» рядом с полным видео.
            let dir = tmpl.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
            args.extend([
                "--split-chapters".into(),
                "-o".into(),
                format!("chapter:{dir}%(title).100B [%(id)s] — главы/%(section_number)02d - %(section_title).100B.%(ext)s"),
            ]);
        }
        if self.s.thumbnail {
            args.extend(["--write-thumbnail".into(), "--convert-thumbnails".into(), "jpg".into()]);
        }
        if self.use_cookies {
            args.extend(self.cookie_args());
        }
        self.p.stage = "video".into();
        self.p.message = "Начинаю загрузку…".into();
        self.emit();
        let mut final_path: Option<String> = None;
        let mut finished = 0;
        let r = self.ytdlp(&args, |me, line| {
            if let Some(rest) = line.strip_prefix("VGDF|") {
                final_path = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("VGDP|") {
                me.on_progress_line(rest, info, &mut finished);
            }
        })?;
        if !r.ok {
            return fail(r.errors.last().cloned().unwrap_or_else(|| "yt-dlp завершился с ошибкой".into()));
        }
        match final_path {
            Some(p) if Path::new(&p).exists() => Ok(PathBuf::from(p)),
            Some(p) => fail(format!("Файл не найден после загрузки: {p}")),
            None => fail(r.errors.last().cloned().unwrap_or_else(|| "yt-dlp не сообщил, куда сохранил файл".into())),
        }
    }

    fn on_progress_line(&mut self, rest: &str, info: &Value, finished: &mut usize) {
        let f: Vec<&str> = rest.split('|').collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("NA");
        let num = |i: usize| get(i).parse::<f64>().ok();
        let status = get(0);
        let kind = if get(6) == "none" { "audio" } else { "video" };
        match status {
            "downloading" => {
                let total = num(2).or(num(3)).unwrap_or(0.0);
                let done = num(1).unwrap_or(0.0);
                self.p.stage = kind.into();
                self.p.percent = if total > 0.0 { (done / total * 100.0).min(100.0) as f32 } else { 0.0 };
                self.p.speed = num(4).map(|s| format!("{}/с", fmt_bytes(s))).unwrap_or_default();
                self.p.eta = num(5).map(fmt_eta).unwrap_or_default();
                let note = if get(7) != "NA" { get(7) } else { get(8) };
                self.p.message = format!(
                    "{} {} · {}",
                    if kind == "audio" { "Звук" } else { "Видео" },
                    note,
                    fmt_bytes(total)
                );
                let _ = info;
                self.emit();
            }
            "finished" => {
                *finished += 1;
                self.p.percent = 100.0;
                self.p.speed.clear();
                self.p.eta.clear();
                // После видео и звука yt-dlp склеивает их — следующая строка прогресса это перекроет.
                self.p.stage = "merge".into();
                self.p.message = "Склеиваю видео и звук…".into();
                self.emit();
            }
            _ => {}
        }
    }

    /// Сразу после скачивания: пересчёт fps, субтитры, копия для монтажа.
    fn finish_video(&mut self, info: &Value, json_path: &Path, path: &Path) -> R<()> {
        let desc = describe(info);
        self.log(if desc.is_empty() {
            format!("Скачано: {}", path.display())
        } else {
            format!("Скачано: {}  [{desc}]", path.display())
        });
        let mut path = path.to_path_buf();
        if self.opts.audio_only {
            self.results.push(path.to_string_lossy().into_owned());
            self.note_video_source(info, &path);
            return check_disk(&self.s.out_dir);
        }
        let mut edit = self.s.edit.clone();
        let fps = probe(&path).fps;
        let max = self.s.max_fps as f64;
        let too_fast = max > 0.0 && fps > max + 0.01;
        // «Вместо оригинала»: один проход ffmpeg — сразу нужный формат (и fps), MKV не остаётся.
        let replace = self.s.edit_replace && edit != "none";
        if replace {
            let ext = if edit == "prores" { "mov" } else { "mp4" };
            let new = path.with_extension(ext);
            let tmp = if new == path { path.with_extension(format!("vgdtmp.{ext}")) } else { new.clone() };
            let label = if too_fast { format!("{} -> {} fps", edit_name(&edit), self.s.max_fps) } else { String::new() };
            match self.make_edit_copy(&path, info, &edit, too_fast.then_some(self.s.max_fps), Some(tmp.clone()), &label) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&path);
                    if tmp != new {
                        let _ = std::fs::rename(&tmp, &new);
                    }
                    path = new;
                }
                Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                Err(DlError::Failed(m)) => {
                    self.errors += 1;
                    self.log(format!("Ошибка: перекодирование, оставил оригинал: {}", first_line(&m)));
                }
            }
            edit = "none".into();
        } else if too_fast {
            // Формата с нужной частотой на YouTube не было — пересчитываем. Результат сразу
            // H.264 MP4 (открывается в AE), поэтому отдельная H.264-копия уже не нужна.
            // Если MP4 уже есть (прошлый запуск прервался), перезаписываем — имя уникально по [id].
            let new = path.with_extension("mp4");
            // Pinterest, VK и др. отдают сразу MP4: ffmpeg не может писать в свой же входной файл —
            // сначала во временный, потом подменяем.
            let tmp = if new == path { path.with_extension("vgdfps.mp4") } else { new.clone() };
            let label = format!("{} -> {} fps", trim_float(fps), self.s.max_fps);
            match self.make_edit_copy(&path, info, "h264", Some(self.s.max_fps), Some(tmp.clone()), &label) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&path);
                    if tmp != new {
                        let _ = std::fs::rename(&tmp, &new);
                    }
                    path = new;
                    if edit == "h264" {
                        edit = "none".into();
                    }
                }
                Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
                // Не вышло пересчитать — оставляем оригинал, видео не теряем.
                Err(DlError::Failed(m)) => {
                    self.errors += 1;
                    self.log(format!("Ошибка: пересчёт fps: {}", first_line(&m)));
                }
            }
        }
        self.results.push(path.to_string_lossy().into_owned());
        self.note_video_source(info, &path);
        if self.s.subs {
            self.download_subs(info, json_path, &path)?;
        }
        if edit != "none" {
            let copy = self.make_edit_copy(&path, info, &edit, None, None, "")?;
            self.results.push(copy.to_string_lossy().into_owned());
        }
        check_disk(&self.s.out_dir)
    }

    fn note_video_source(&self, info: &Value, path: &Path) {
        if self.s.source_log {
            let site = str_of(info, &["extractor_key", "extractor"]).if_empty("сайт");
            note_source(path, &site, &str_of(info, &["webpage_url", "original_url"]), &self.asked);
        }
    }

    // ---------- субтитры ----------

    /// Какие дорожки просить. Автоперевод (например, ru из английских автосубтитров) YouTube
    /// чаще всего режет ошибкой 429 — поэтому из автосубтитров берём только язык оригинала (`xx-orig`).
    fn pick_subs(&self, info: &Value) -> Vec<String> {
        let manual: Vec<String> = keys(info.get("subtitles"));
        let auto: Vec<String> = keys(info.get("automatic_captions"));
        let mut out = Vec::new();
        for lang in self.s.sub_lang_list() {
            let m: Vec<&String> =
                manual.iter().filter(|k| *k == &lang || k.starts_with(&format!("{lang}-"))).collect();
            if !m.is_empty() {
                out.extend(m.into_iter().cloned());
            } else if self.s.auto_subs {
                let orig = format!("{lang}-orig");
                if auto.contains(&orig) {
                    out.push(orig);
                }
            }
        }
        out.dedup();
        out
    }

    fn download_subs(&mut self, info: &Value, json_path: &Path, video: &Path) -> R<()> {
        let langs = self.pick_subs(info);
        if langs.is_empty() {
            self.log(format!("Субтитры: нет дорожек {} (автоперевод не берём).", self.s.sub_langs));
            return Ok(());
        }
        let dir = video.parent().unwrap_or(Path::new(".")).to_path_buf();
        let stem = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.p.stage = "subs".into();
        self.p.percent = 0.0;
        self.p.speed.clear();
        self.p.eta.clear();
        self.p.message = "Субтитры…".into();
        self.emit();
        let before = sub_files(&dir, &stem, "srt");
        let mut args: Vec<String> = vec![
            "--load-info-json".into(), json_path.to_string_lossy().into_owned(),
            "--skip-download".into(), "--write-subs".into(),
            "--sub-langs".into(), langs.join(","),
            "--sleep-subtitles".into(), "2".into(),
            "-P".into(), format!("home:{}", dir.display()),
            "-o".into(), format!("{}.%(ext)s", stem.replace('%', "%%")),
            "--ignore-errors".into(),
        ];
        if self.s.auto_subs {
            args.push("--write-auto-subs".into());
        }
        if self.use_cookies {
            args.extend(self.cookie_args());
        }
        let r = self.ytdlp(&args, |_, _| {})?;
        for e in &r.errors {
            if e.contains("429") {
                self.log("Субтитры: YouTube временно ограничил запросы (429), видео сохранено.");
                break;
            }
        }
        // С --skip-download конвертер yt-dlp не запускается — переводим .vtt в .srt сами.
        for vtt in sub_files(&dir, &stem, "vtt") {
            let srt = vtt.with_extension("srt");
            let ok = command(tool("ffmpeg"))
                .args(["-v", "error", "-nostdin", "-y", "-i"])
                .arg(&vtt)
                .arg(&srt)
                .output()
                .is_ok_and(|o| o.status.success());
            if ok && srt.exists() {
                let _ = std::fs::remove_file(&vtt);
            }
        }
        // «название.ru-orig.srt» → «название.ru.srt»
        for srt in sub_files(&dir, &stem, "srt") {
            let name = srt.file_name().unwrap().to_string_lossy().into_owned();
            if let Some(base) = name.strip_suffix("-orig.srt") {
                let dst = dir.join(format!("{base}.srt"));
                if !dst.exists() {
                    let _ = std::fs::rename(&srt, &dst);
                }
            }
        }
        let after = sub_files(&dir, &stem, "srt");
        let mut got: Vec<String> = after
            .iter()
            .filter(|f| !before.contains(f))
            .filter_map(|f| f.file_name())
            .map(|n| {
                let n = n.to_string_lossy();
                n[stem.len() + 1..n.len() - 4].to_string()
            })
            .collect();
        got.sort();
        if got.is_empty() {
            self.log("Субтитры: не скачались (YouTube мог временно ограничить запросы; видео сохранено).");
        } else {
            self.log(format!("Субтитры: {}", got.join(", ")));
        }
        Ok(())
    }

    // ---------- копия для монтажа ----------

    fn make_edit_copy(
        &mut self,
        src: &Path,
        info: &Value,
        fmt: &str,
        fps: Option<u32>,
        dst: Option<PathBuf>,
        label: &str,
    ) -> R<PathBuf> {
        let m = probe(src);
        let mut vf: Vec<String> = Vec::new();
        if let Some(f) = fps {
            vf.push(format!("fps={f}"));
        }
        if m.hdr {
            // HDR (PQ/HLG, BT.2020) → SDR BT.709, иначе в AE/Premiere будут блёклые цвета.
            vf.push("zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv".into());
        }
        let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (dst, codec, acodec): (PathBuf, Vec<&str>, Vec<&str>) = if fmt == "prores" {
            vf.push("format=yuv422p10le".into());
            (
                dst.unwrap_or_else(|| src.with_file_name(format!("{stem}_edit.mov"))),
                vec!["-c:v", "prores_ks", "-profile:v", "3", "-vendor", "apl0"],
                vec!["-c:a", "pcm_s16le"],
            )
        } else {
            vf.push("format=yuv420p".into());
            (
                dst.unwrap_or_else(|| src.with_file_name(format!("{stem}_edit.mp4"))),
                vec!["-c:v", "libx264", "-preset", "medium", "-crf", "16", "-profile:v", "high", "-movflags", "+faststart"],
                vec!["-c:a", "aac", "-b:a", "320k"],
            )
        };
        let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into(), "-i".into(), src.to_string_lossy().into_owned(), "-map".into(), "0:v:0".into()];
        if m.has_audio {
            args.extend(["-map".into(), "0:a:0".into()]);
        }
        args.extend(["-vf".into(), vf.join(",")]);
        args.extend(codec.iter().map(|s| s.to_string()));
        // Постоянная частота кадров — AE и Premiere не любят VFR.
        args.extend(["-fps_mode", "cfr", "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"].map(String::from));
        if m.has_audio {
            args.extend(acodec.iter().map(|s| s.to_string()));
            args.extend(["-ar".into(), "48000".into()]);
        }
        args.extend(["-progress".into(), "pipe:1".into(), "-nostats".into(), dst.to_string_lossy().into_owned()]);

        let label = format!("{}{}", if label.is_empty() { edit_name(fmt) } else { label }, if m.hdr { " (HDR→SDR)" } else { "" });
        self.p.stage = "edit".into();
        self.p.percent = 0.0;
        self.p.speed.clear();
        self.p.eta.clear();
        self.p.title = str_of(info, &["title"]).if_empty(&stem);
        self.p.message = format!("Перекодирую: {label}");
        self.emit();
        self.log(format!("Перекодирую: {} ({label})", dst.file_name().unwrap_or_default().to_string_lossy()));

        let duration = m.duration;
        let mut speed = String::new();
        let mut tail: Vec<String> = Vec::new();
        let (ok, _) = self.spawn_lines(tool("ffmpeg"), &args, true, |me, line, _is_err| {
            let (key, val) = match line.split_once('=') {
                Some((k, v)) if !k.contains(' ') => (k.trim(), v.trim()),
                _ => {
                    tail.push(line.to_string());
                    if tail.len() > 30 {
                        tail.remove(0);
                    }
                    return;
                }
            };
            if key == "speed" {
                speed = val.to_string();
            } else if key == "out_time_us" && duration > 0.0 {
                if let Ok(us) = val.parse::<f64>() {
                    let done = us / 1e6;
                    let sp: f64 = speed.trim_end_matches('x').trim().parse().unwrap_or(0.0);
                    me.p.percent = (done / duration * 100.0).clamp(0.0, 100.0) as f32;
                    me.p.speed = if speed == "N/A" { String::new() } else { speed.clone() };
                    me.p.eta = if sp > 0.0 { fmt_eta((duration - done) / sp) } else { String::new() };
                    me.emit();
                }
            }
        })?;
        if self.cancel.is_cancelled() {
            let _ = std::fs::remove_file(&dst);
            return Err(DlError::Cancelled(String::new()));
        }
        if !ok {
            let _ = std::fs::remove_file(&dst);
            let err = tail.join("\n");
            let start = err.len().saturating_sub(1500);
            let start = (start..err.len()).find(|i| err.is_char_boundary(*i)).unwrap_or(err.len());
            return fail(format!("ffmpeg завершился с ошибкой:\n{}", &err[start..]));
        }
        Ok(dst)
    }

    // ---------- запуск yt-dlp ----------

    fn format_args(&self) -> Vec<String> {
        // Лучшее видео + лучший звук; ограничения качества — через сортировку:
        // лучшее из того, что не выше лимита. res = меньшая сторона кадра (вертикальные 1080x1920 = 1080p).
        if self.opts.audio_only {
            return vec!["-f".to_string(), "bestaudio/best".to_string()];
        }
        let mut a = vec!["-f".to_string(), "bestvideo*+bestaudio/best".to_string()];
        let mut sort = Vec::new();
        if self.s.max_height > 0 {
            sort.push(format!("res:{}", self.s.max_height));
        }
        if self.s.max_fps > 0 {
            sort.push(format!("fps:{}", self.s.max_fps));
        }
        if self.tiktok {
            // TikTok отдаёт HEVC и H.264 одного разрешения — берём H.264: его открывают все программы.
            if sort.is_empty() {
                sort.push("res".into());
            }
            sort.push("vcodec:h264".into());
        }
        if !sort.is_empty() {
            a.extend(["-S".to_string(), sort.join(",")]);
        }
        a
    }

    fn cookie_args(&self) -> Vec<String> {
        let s = &self.s;
        if s.cookies == "file" && !s.cookies_file.is_empty() {
            return vec!["--cookies".into(), s.cookies_file.clone()];
        }
        if !s.cookies.is_empty() && s.cookies != "none" && s.cookies != "file" {
            return vec!["--cookies-from-browser".into(), s.cookies.clone()];
        }
        Vec::new()
    }

    /// `yt-dlp -j/-J`: без куки, а если видео требует входа — повтор с куки браузера.
    /// (С куки вошедшего аккаунта YouTube отдаёт урезанный набор форматов — поэтому не сразу.)
    fn json_with_login_retry(&mut self, args: &[String], url: &str) -> R<Value> {
        let mut a = args.to_vec();
        a.extend(["--sleep-requests".into(), "0.5".into()]);
        if self.use_cookies {
            a.extend(self.cookie_args());
        }
        a.push("--".into());
        a.push(url.into());
        let r = self.ytdlp(&a, |_, _| {})?;
        if r.ok {
            return parse_json(&r.stdout);
        }
        let err = r.errors.join("\n");
        let original = r.errors.last().cloned().unwrap_or_else(|| "yt-dlp завершился с ошибкой".into());
        if !self.use_cookies && !self.cookies_broken && needs_login(&err) && !self.cookie_args().is_empty() {
            self.log("Видео требует входа в аккаунт — повторяю с куки браузера.");
            self.use_cookies = true;
            match self.json_with_login_retry(args, url) {
                // Куки сработали — дальше по этой ссылке сразу с ними.
                Ok(v) => return Ok(v),
                Err(DlError::Failed(m)) => {
                    self.use_cookies = false;
                    if m.to_lowercase().contains("cookie") {
                        // Браузер не установлен или куки не читаются: не пытаемся на каждом видео.
                        self.cookies_broken = true;
                        self.log(format!(
                            "Внимание: куки из «{}» недоступны — дальше без куки ({})",
                            self.s.cookies,
                            m.chars().take(160).collect::<String>()
                        ));
                    }
                    return fail(original);
                }
                Err(c) => return Err(c),
            }
        }
        fail(original)
    }

    /// Запускает yt-dlp; `on_line` получает строки stdout и stderr.
    fn ytdlp(&mut self, args: &[String], mut on_line: impl FnMut(&mut Self, &str)) -> R<Run> {
        let mut stdout = String::new();
        let mut errors = Vec::new();
        let mut last_other = String::new();
        // --encoding: иначе пути с кириллицей yt-dlp печатает в кодировке консоли (cp1251).
        let mut full: Vec<String> = vec!["--encoding".into(), "utf-8".into()];
        full.extend_from_slice(args);
        let (ok, _) = self.spawn_lines(tool("yt-dlp"), &full, false, |me, line, is_err| {
            if line.starts_with("VGDP|") || line.starts_with("VGDF|") {
                on_line(me, line);
                return;
            }
            if !is_err {
                stdout.push_str(line);
                stdout.push('\n');
                on_line(me, line);
                return;
            }
            if let Some(m) = line.strip_prefix("ERROR: ") {
                errors.push(m.to_string());
            } else if let Some(m) = line.strip_prefix("WARNING: ") {
                // Шумные предупреждения не нужны в журнале.
                if !m.contains("ffprobe")
                    && !m.contains("Falling back")
                    && !m.contains("Unable to download video thumbnail")
                    && !m.contains("initial data")
                    // прямые ссылки на звук (VK): название всё равно берётся из списка
                    && !m.contains("failed to obtain \"title\"")
                {
                    me.log(format!("Предупреждение: {m}"));
                }
            } else if !line.trim().is_empty() && !line.starts_with('[') {
                last_other = line.to_string();
                me.log(line.to_string());
            }
        })?;
        // Упал без строки ERROR (например, сбой сети или процесс прервали) — покажем хоть последнюю строку.
        if !ok && errors.is_empty() && !last_other.is_empty() {
            errors.push(last_other);
        }
        Ok(Run { ok, stdout, errors })
    }

    /// Запуск процесса с построчным чтением stdout/stderr (stderr читается всегда — иначе
    /// переполненный буфер канала подвешивает ffmpeg). Возвращает (успех, код).
    fn spawn_lines(
        &mut self,
        program: PathBuf,
        args: &[String],
        merge_stderr: bool,
        mut on_line: impl FnMut(&mut Self, &str, bool),
    ) -> R<(bool, Option<i32>)> {
        self.check_cancel()?;
        let mut child: Child = command(&program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DlError::Failed(format!("Не удалось запустить {}: {e}", program.display())))?;
        let pid = child.id();
        self.cancel.child.lock().unwrap().push(pid);
        let (tx, rx) = mpsc::channel::<(bool, String)>();
        let mut readers = Vec::new();
        for (is_err, pipe) in [(false, child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>)),
                               (true, child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>))] {
            let Some(pipe) = pipe else { continue };
            let tx = tx.clone();
            let flag = is_err && !merge_stderr;
            readers.push(std::thread::spawn(move || read_lines(pipe, flag, tx)));
        }
        drop(tx);
        for (is_err, line) in rx {
            on_line(self, &line, is_err);
        }
        for r in readers {
            let _ = r.join();
        }
        let status = child.wait();
        self.cancel.child.lock().unwrap().retain(|p| *p != pid);
        self.check_cancel()?;
        let status = status.map_err(|e| DlError::Failed(e.to_string()))?;
        Ok((status.success(), status.code()))
    }
}

/// Читает поток и шлёт строки (разделители — \n и \r).
fn read_lines(pipe: Box<dyn Read + Send>, is_err: bool, tx: mpsc::Sender<(bool, String)>) {
    let mut r = BufReader::new(pipe);
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = match r.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &chunk[..n] {
            if b == b'\n' || b == b'\r' {
                if !buf.is_empty() {
                    let _ = tx.send((is_err, String::from_utf8_lossy(&buf).into_owned()));
                    buf.clear();
                }
            } else {
                buf.push(b);
            }
        }
    }
    if !buf.is_empty() {
        let _ = tx.send((is_err, String::from_utf8_lossy(&buf).into_owned()));
    }
}

// ---------- мелочи ----------

trait IfEmpty {
    fn if_empty(self, other: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, other: &str) -> String {
        if self.is_empty() {
            other.to_string()
        } else {
            self
        }
    }
}

fn str_of(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| v.get(*k).and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn keys(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(300).collect()
}

fn parse_json(stdout: &str) -> R<Value> {
    // -J печатает один объект; на всякий случай берём последнюю непустую строку-объект.
    let text = stdout.trim();
    serde_json::from_str(text)
        .or_else(|_| {
            text.lines().rev().find(|l| l.trim_start().starts_with('{')).map(serde_json::from_str).unwrap_or(Ok(Value::Null))
        })
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| DlError::Failed("yt-dlp вернул непонятный ответ".into()))
}

/// Ошибки, при которых стоит повторить с куки браузера.
pub fn needs_login(err: &str) -> bool {
    let e = err.to_lowercase();
    [
        "sign in", "log in", "login", "age-restrict", "age restrict", "inappropriate", "members",
        "join this channel", "private video", "not a bot", "premium", "logged-in", "logged in",
        "authentication", "registered users", "cookies",
    ]
    .iter()
    .any(|k| e.contains(k))
}

fn describe(info: &Value) -> String {
    let mut parts = Vec::new();
    if let (Some(w), Some(h)) = (info.get("width").and_then(Value::as_u64), info.get("height").and_then(Value::as_u64)) {
        parts.push(format!("{w}x{h}"));
    }
    if let Some(f) = info.get("fps").and_then(Value::as_f64) {
        parts.push(format!("{} fps", trim_float(f)));
    }
    let vc = str_of(info, &["vcodec"]);
    if !vc.is_empty() && vc != "none" {
        parts.push(vc.split('.').next().unwrap_or("").to_string());
    }
    let dr = str_of(info, &["dynamic_range"]);
    if !dr.is_empty() && dr != "SDR" {
        parts.push(dr);
    }
    let ac = str_of(info, &["acodec"]);
    if !ac.is_empty() && ac != "none" {
        parts.push(ac.split('.').next().unwrap_or("").to_string());
    }
    parts.join(", ")
}

fn trim_float(f: f64) -> String {
    let s = format!("{f:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn archive_key(e: &Entry) -> String {
    format!("{} {}", e.extractor, e.id)
}

fn archive_key_info(info: &Value, e: &Entry) -> String {
    let ex = str_of(info, &["extractor_key"]).to_lowercase();
    let id = str_of(info, &["id"]);
    format!("{} {}", if ex.is_empty() { e.extractor.clone() } else { ex }, if id.is_empty() { e.id.clone() } else { id })
}

fn read_archive(out: &Path) -> HashSet<String> {
    std::fs::read_to_string(out.join(ARCHIVE_NAME))
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn append_archive(out: &Path, key: &str) {
    crate::appinfo::append_hidden(&out.join(ARCHIVE_NAME), key);
}

pub const SOURCE_LOG: &str = "_откуда скачано.txt";

/// Строка в «_откуда скачано.txt» в папке файла: когда, что, откуда взято, по какой ссылке.
fn note_source(file: &Path, from: &str, url: &str, asked: &str) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock();
    let Some(dir) = file.parent() else { return };
    let log = dir.join(SOURCE_LOG);
    let new = !log.exists();
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) else { return };
    if new {
        let _ = writeln!(f, "\u{feff}Дата | Файл | Откуда | Ссылка | Что просили скачать");
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let asked = if asked == url { "" } else { asked };
    let _ = writeln!(f, "{} | {name} | {from} | {url} | {asked}", local_stamp());
}

/// «2026-10-27 14:05» по местному времени.
#[cfg(windows)]
fn local_stamp() -> String {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        dow: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute)
}

#[cfg(not(windows))]
fn local_stamp() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("unix {s}")
}

/// Файлы «<stem>.<язык>.<ext>» рядом с видео.
fn sub_files(dir: &Path, stem: &str, ext: &str) -> Vec<PathBuf> {
    let prefix = format!("{stem}.");
    let suffix = format!(".{ext}");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    n.len() > prefix.len() + suffix.len() && n.starts_with(&prefix) && n.ends_with(&suffix)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Пустые временные папки, которые yt-dlp оставляет в `.parts`.
fn remove_empty_dirs(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    let mut empty = true;
    for e in rd.flatten() {
        let p = e.path();
        if !(p.is_dir() && remove_empty_dirs(&p)) {
            empty = false;
        }
    }
    empty && std::fs::remove_dir(dir).is_ok()
}

/// Сколько дней назад загружено видео (upload_date «20261025»).
fn upload_age_days(date: &str) -> Option<i64> {
    if date.len() != 8 {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (date[..4].parse().ok()?, date[4..6].parse().ok()?, date[6..].parse().ok()?);
    // Дни от 1970-01-01 (алгоритм «days from civil»).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64 / 86400;
    Some(now - days)
}

fn rand_u64() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let mut x = t.as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

// ---------- место на диске ----------

#[cfg(windows)]
pub fn free_gb(folder: &str) -> Option<f64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    let mut root = PathBuf::from(folder);
    while !root.exists() {
        match root.parent() {
            Some(p) if p != root => root = p.to_path_buf(),
            _ => break,
        }
    }
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then(|| avail as f64 / (1u64 << 30) as f64)
}

#[cfg(not(windows))]
pub fn free_gb(_folder: &str) -> Option<f64> {
    None
}

fn check_disk(folder: &str) -> R<()> {
    match free_gb(folder) {
        Some(free) if free < MIN_FREE_GB => Err(DlError::Cancelled(format!(
            "На диске осталось {free:.1} ГБ (минимум {MIN_FREE_GB}) — загрузка остановлена."
        ))),
        _ => Ok(()),
    }
}

// ---------- разложить уже скачанное ----------

/// Сведения о видео по id: один вызов yt-dlp на пачку из 25 видео.
pub fn fetch_infos(
    ids: &[String],
    log: &dyn Fn(String),
    cancel: Option<&CancelHandle>,
) -> Result<std::collections::HashMap<String, Value>, String> {
    let mut out = std::collections::HashMap::new();
    let total = ids.len();
    for (n, chunk) in ids.chunks(25).enumerate() {
        if cancel.is_some_and(CancelHandle::is_cancelled) {
            break;
        }
        log(format!("[{}–{} из {total}] Узнаю каналы и темы…", n * 25 + 1, n * 25 + chunk.len()));
        let mut c = command(tool("yt-dlp"));
        c.args(["--encoding", "utf-8", "-j", "--skip-download", "--ignore-errors", "--no-warnings", "--no-playlist", "--sleep-requests", "0.5", "--"]);
        c.args(chunk.iter().map(|id| format!("https://www.youtube.com/watch?v={id}")));
        let mut child = c
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Не удалось запустить yt-dlp: {e}"))?;
        if let Some(h) = cancel {
            h.child.lock().unwrap().push(child.id());
        }
        let stderr = child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>);
        let (tx, rx) = mpsc::channel();
        let reader = stderr.map(|p| std::thread::spawn(move || read_lines(p, true, tx)));
        let mut stdout = String::new();
        if let Some(mut so) = child.stdout.take() {
            let mut buf = Vec::new();
            let _ = so.read_to_end(&mut buf);
            stdout = String::from_utf8_lossy(&buf).into_owned();
        }
        if let Some(r) = reader {
            let _ = r.join();
        }
        let _ = child.wait();
        if let Some(h) = cancel {
            h.child.lock().unwrap().clear();
        }
        for (_, line) in rx.try_iter() {
            if let Some(m) = line.strip_prefix("ERROR: ") {
                log(format!("  пропускаю: {}", m.chars().take(160).collect::<String>()));
            }
        }
        for line in stdout.lines().filter(|l| l.starts_with('{')) {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                let id = str_of(&v, &["id"]);
                out.insert(id, v);
            }
        }
    }
    Ok(out)
}

/// Ищет в корне папки файлы с [id] в имени и решает, куда их переложить (сначала план, потом перенос).
pub fn plan_existing(
    root: &Path,
    log: &dyn Fn(String),
    cancel: Option<&CancelHandle>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let groups = crate::sort::group_root_files(root);
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = groups.iter().map(|(id, _)| id.clone()).collect();
    let infos = fetch_infos(&ids, log, cancel)?;
    let mut sorter = Sorter::new(root);
    let mut plan = Vec::new();
    for (id, files) in groups {
        let Some(info) = infos.get(&id) else { continue };
        let (topic, chan) = sorter.target_dir(info);
        for f in files {
            let name = f.file_name().unwrap_or_default().to_owned();
            plan.push((f, root.join(&topic).join(&chan).join(name)));
        }
    }
    Ok(plan)
}

// ---------- обновление yt-dlp ----------

/// Версия yt-dlp, которой пользуется программа.
pub fn ytdlp_version() -> Option<String> {
    let o = command(tool("yt-dlp")).arg("--version").output().ok()?;
    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Обновляет yt-dlp (YouTube часто ломает старые версии). Копия кладётся в
/// `%APPDATA%\VeryGoodDownloader\bin` — туда можно писать без прав администратора,
/// и она находится раньше встроенной.
pub fn update_ytdlp(log: &dyn Fn(String)) -> Result<String, String> {
    let user_bin = data_dir().join("bin");
    let target = user_bin.join("yt-dlp.exe");
    let current = tool("yt-dlp");
    if !target.exists() {
        std::fs::create_dir_all(&user_bin).map_err(|e| e.to_string())?;
        if current.is_file() {
            std::fs::copy(&current, &target).map_err(|e| format!("не удалось скопировать yt-dlp: {e}"))?;
        } else {
            return Err("yt-dlp.exe не найден".into());
        }
    }
    let before = ytdlp_version().unwrap_or_default();
    log("Проверяю новую версию yt-dlp…".into());
    let o = command(&target).arg("-U").output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    if !o.status.success() {
        return Err(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ошибка").to_string());
    }
    let after = command(&target)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if vtuple(&after) > vtuple(&before) {
        log(format!("yt-dlp: {before} → {after}"));
    } else {
        log(format!("yt-dlp уже последней версии: {after}"));
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login() {
        assert!(needs_login("Sign in to confirm your age. This video may be inappropriate"));
        assert!(needs_login("Private video. Sign in if you've been granted access"));
        assert!(!needs_login("HTTP Error 403: Forbidden"));
    }
}
