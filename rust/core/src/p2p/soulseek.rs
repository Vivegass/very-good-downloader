//! Soulseek: подключение один раз на процесс, поиск FLAC и докачиваемые передачи.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use soulseek_client::soulseek_proto::peer::{File as SlskFile, SearchResponse};
use soulseek_client::transfer;
use soulseek_client::{Client, Config};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::{
    cancellable, serve_stream, AudioCandidate, AudioSourceProvider, AudioStream, DownloadRequest,
    DownloadedAudio, P2pError, ProviderKind, SearchQuery, SourceLocator,
};
use crate::settings::Settings;

/// Сколько ждать новых ответов поиска, прежде чем решить, что их больше не будет.
/// Ответы с MP3 от других пиров продолжают приходить и продлевают idle-таймер —
/// поэтому он короткий: три секунды тишины уже означают «больше ничего полезного».
const SEARCH_IDLE: Duration = Duration::from_secs(3);
/// Потолок всего поиска: даже если пиры отвечают, дольше не ждём.
const SEARCH_MAX: Duration = Duration::from_secs(8);
/// Сколько добирать кандидатов после того, как уже набралось достаточно хороших:
/// за это время могут прийти более быстрые пиры.
const SEARCH_GRACE: Duration = Duration::from_secs(2);
/// Пир считается быстрым, если отдаёт хотя бы столько (байт/с, поле Soulseek).
const GOOD_SPEED: u32 = 200_000;
/// Сколько ждать место в очереди пира, прежде чем перейти к следующему кандидату.
const QUEUE_WAIT: Duration = Duration::from_secs(12);
/// Watchdog передачи: столько должен качать активный пир, иначе попытка отменяется
/// (средняя скорость ниже `STALLED_SPEED` или байты не растут столько же).
const WATCHDOG_GRACE: Duration = Duration::from_secs(6);
const WATCHDOG_TICK: Duration = Duration::from_millis(500);
const STALLED_SPEED: f64 = 50.0 * 1024.0;
const STREAM_CACHE: &str = "stream-cache";

pub struct SoulseekProvider {
    client: Client,
    username: String,
    password: String,
    connect_lock: AsyncMutex<()>,
    cache_dir: PathBuf,
}

/// Состояние передачи для watchdog'а скорости, обновляется из reporter-колбэка.
#[derive(Default)]
struct WatchState {
    bytes: AtomicU64,
    speed: AtomicU64,
    /// момент перехода в активное скачивание (InProgress) — None, пока не началось.
    started: Mutex<Option<Instant>>,
    /// момент последнего роста байтов.
    last_growth: Mutex<Option<Instant>>,
}

impl WatchState {
    /// Вызывается из reporter-колбэка на каждый чанк.
    fn observe(&self, t: &transfer::Transfer) {
        let now = Instant::now();
        if matches!(t.state, transfer::State::InProgress) {
            let mut started = self.started.lock().unwrap();
            if started.is_none() {
                *started = Some(now);
                *self.last_growth.lock().unwrap() = Some(now);
            }
        }
        let prev = self.bytes.swap(t.bytes_transferred, AtomicOrdering::Relaxed);
        if t.bytes_transferred > prev {
            *self.last_growth.lock().unwrap() = Some(now);
        }
        self.speed.store(t.current_speed.max(0.0) as u64, AtomicOrdering::Relaxed);
    }
}

impl SoulseekProvider {
    pub async fn new(settings: &Settings, cache_dir: PathBuf) -> Result<Self, P2pError> {
        let username = settings.slsk_username.trim().to_string();
        let password = settings.slsk_password.clone();
        if username.is_empty() || password.is_empty() {
            return Err(P2pError::Configuration("укажите логин и пароль Soulseek в настройках музыки".into()));
        }
        let config = Config::builder()
            .listen_port(settings.slsk_listen_port)
            .transfer_connection(|c| c.inactivity_timeout(Duration::from_secs(10)))
            .build()
            .map_err(|e| P2pError::Configuration(format!("Soulseek: {e}")))?;
        tokio::fs::create_dir_all(cache_dir.join(STREAM_CACHE)).await?;
        let client = Client::new(config);
        // Проброс порта на роутере: без него NAT ломает «прямые» передачи, и часть
        // пиров не может отдать файл. Тихий фоллбэк — работаем как раньше.
        forward_port_upnp(settings.slsk_listen_port);
        Ok(Self { client, username, password, connect_lock: AsyncMutex::new(()), cache_dir })
    }

    async fn ensure_connected(&self, cancel: &CancellationToken) -> Result<(), P2pError> {
        let _guard = self.connect_lock.lock().await;
        if self.client.state().is_logged_in() {
            return Ok(());
        }
        info!("connecting to Soulseek");
        match cancellable(self.client.connect(&self.username, &self.password), cancel).await {
            Ok(Ok(())) => {
                info!("Soulseek connected");
                Ok(())
            }
            Ok(Err(e)) => Err(map_client_error("Soulseek: не удалось подключиться", e)),
            Err(e) => Err(e),
        }
    }

    fn candidate(response: &SearchResponse, file: &SlskFile) -> Option<AudioCandidate> {
        if file.derived_extension() != "flac" || file.size == 0 {
            return None;
        }
        let remote_path = file.filename.as_str().to_string();
        let id = format!("slsk:{}:{}:{}", response.username, remote_path, file.size);
        Some(AudioCandidate {
            id,
            provider: ProviderKind::Soulseek,
            title: remote_path.rsplit(['\\', '/']).next().unwrap_or(&remote_path).to_string(),
            remote_path: remote_path.clone(),
            size: file.size,
            duration: file.length().map(|v| Duration::from_secs(v.into())),
            bit_rate: file.bit_rate(),
            sample_rate: file.sample_rate(),
            bit_depth: file.bit_depth(),
            free_slot: response.has_free_upload_slot,
            queue_length: soulseek_client::soulseek_proto::queue_count(response.queue_length),
            upload_speed: Some(response.upload_speed),
            locator: SourceLocator::Soulseek {
                username: response.username.clone(),
                remote_filename: remote_path,
                size: file.size,
            },
        })
    }

    fn cache_path(&self, username: &str, filename: &str, size: u64) -> PathBuf {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(format!("{username}\0{filename}\0{size}").as_bytes());
        self.cache_dir.join(STREAM_CACHE).join(format!("{}.flac.part", hex(&hash[..16])))
    }

    /// Скачивание одного файла с watchdog'ом скорости. `resume` — дописывать
    /// существующий частичный файл; безопасно только для пути, уникального
    /// для этого кандидата (username+filename+size, как `cache_path`).
    async fn download_file(&self, username: &str, filename: &str, size: u64, request: &DownloadRequest, resume: bool) -> Result<(), P2pError> {
        self.ensure_connected(&request.cancel).await?;
        let destination = &request.destination;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let offset = if resume {
            tokio::fs::metadata(destination).await.map(|m| m.len()).unwrap_or(0).min(size)
        } else {
            0
        };
        if offset == size && size > 0 {
            return Ok(());
        }
        let progress = request.progress.clone();
        let watch = Arc::new(WatchState::default());
        let watch_for_cb = watch.clone();
        let callback = move |t: &transfer::Transfer, _chunk: transfer::Chunk| {
            watch_for_cb.observe(t);
            if let Some(cb) = &progress {
                cb(super::TransferProgress {
                    downloaded: t.bytes_transferred,
                    total: t.size,
                    bytes_per_second: Some(t.current_speed.max(0.0) as u64),
                    message: format!("Soulseek: {}", t.state),
                });
            }
        };
        let download = self.client
            .download(username.to_string(), filename.to_string())
            .size(size)
            .start_offset(offset)
            .queued_timeout(QUEUE_WAIT)
            .to_file(destination.clone())
            .reporter(callback);
        let handle = match download.start().await {
            Ok(h) => h,
            Err(e) => return Err(map_client_error("Soulseek: загрузка не удалась", e)),
        };
        // Watchdog: снимает только эту попытку (control.cancel), родительский
        // request.cancel не трогает — задание продолжится со следующего пира.
        let control = handle.control();
        let (stall_tx, stall_rx) = tokio::sync::oneshot::channel::<()>();
        let watchdog = tokio::spawn(watchdog_task(watch, stall_tx));
        let result = tokio::select! {
            biased;
            _ = request.cancel.cancelled() => {
                control.cancel();
                Err(P2pError::Cancelled)
            }
            result = handle => result.map_err(|e| map_client_error("Soulseek: загрузка не удалась", e)),
            _ = stall_rx => {
                control.cancel();
                Err(P2pError::Timeout("Soulseek: пир отдаёт слишком медленно".into()))
            }
        };
        watchdog.abort();
        match result {
            Ok(t) if t.state.succeeded() => Ok(()),
            Ok(t) => Err(P2pError::Unavailable(format!("Soulseek: передача завершилась: {}", t.state))),
            Err(e) => Err(e),
        }
    }

    async fn open_cached_stream(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioStream, P2pError> {
        let SourceLocator::Soulseek { username, remote_filename, size } = locator.clone() else {
            return Err(P2pError::InvalidInput("не Soulseek-источник".into()));
        };
        self.ensure_connected(&cancel).await?;
        let path = self.cache_path(&username, &remote_filename, size);
        let actual = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
        if actual > size {
            tokio::fs::remove_file(&path).await?;
        }
        let server = serve_stream(
            path.clone(),
            size,
            "audio/flac",
            remote_filename.clone(),
            ProviderKind::Soulseek,
            cancel.clone(),
        ).await?;
        let url = server.url();
        let title = remote_filename.rsplit(['\\', '/']).next().unwrap_or(&remote_filename).to_string();
        // Один writer на файл; второй stream того же файла ждёт первого на download mutex.
        let download_lock = stream_download_lock(path.clone()).await;
        let guard = download_lock.lock_owned().await;
        if tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0) < size {
            let request = DownloadRequest {
                locator: locator.clone(),
                destination: path,
                progress: None,
                cancel: cancel.clone(),
            };
            // Не ждём весь файл: HTTP handler читает имеющийся префикс. Если
            // передача упала, конец диапазона сообщит об этом вместо нулевых байт.
            let this = self.clone_arc();
            tokio::spawn(async move {
                let _guard = guard;
                if let Err(e) = this.download_file(&username, &remote_filename, size, &request, true).await {
                    if !e.is_cancelled() {
                        warn!(error = %e, "Soulseek stream download stopped");
                    }
                }
            });
        }
        Ok(AudioStream { url, size, mime: "audio/flac", provider: ProviderKind::Soulseek, title, server })
    }

    fn clone_arc(&self) -> Arc<Self> {
        // Provider всегда создаётся в Arc в HybridEngine. Для фонового cache writer
        // используется только Clone клиента и пути, а не повторный вход.
        Arc::new(Self {
            client: self.client.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            connect_lock: AsyncMutex::new(()),
            cache_dir: self.cache_dir.clone(),
        })
    }
}

/// Фоновая проверка скорости: после `WATCHDOG_GRACE` активного скачивания —
/// средняя скорость ниже `STALLED_SPEED` или байты не растут столько же — сигнал.
async fn watchdog_task(watch: Arc<WatchState>, stall: tokio::sync::oneshot::Sender<()>) {
    loop {
        tokio::time::sleep(WATCHDOG_TICK).await;
        let started = *watch.started.lock().unwrap();
        let Some(started) = started else { continue };
        if started.elapsed() < WATCHDOG_GRACE {
            continue;
        }
        let speed = watch.speed.load(AtomicOrdering::Relaxed) as f64;
        let stalled = speed < STALLED_SPEED;
        let frozen = watch.last_growth.lock().unwrap().map(|t| t.elapsed() >= WATCHDOG_GRACE).unwrap_or(true);
        if stalled || frozen {
            debug!(speed, "Soulseek transfer too slow, switching peer");
            let _ = stall.send(());
            return;
        }
    }
}

async fn stream_download_lock(path: PathBuf) -> Arc<AsyncMutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    let map = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock().unwrap().entry(path).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone()
}

/// Проброс TCP-порта Soulseek на роутере по UPnP (в soulseek-client его нет).
/// Фоновая задача на весь процесс: librqbit-upnp сам продлевает аренду.
/// Тихий фоллбэк: нет UPnP — работаем без проброса, соединения инициируем сами.
fn forward_port_upnp(port: u16) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if port == 0 || STARTED.swap(true, AtomicOrdering::Relaxed) {
        return;
    }
    std::thread::Builder::new()
        .name("slsk-upnp".into())
        .spawn(move || {
            crate::p2p::block_on(async {
                match librqbit_upnp::UpnpPortForwarder::new(vec![port], None, None) {
                    Ok(forwarder) => forwarder.run_forever().await,
                    Err(e) => debug!(error = format!("{e:#}"), "Soulseek UPnP port forward unavailable"),
                }
            });
        })
        .map(|_| ())
        .unwrap_or_else(|e| debug!(error = %e, "Soulseek UPnP thread not started"));
}

#[async_trait]
impl AudioSourceProvider for SoulseekProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Soulseek
    }

    async fn search(&self, query: &SearchQuery, cancel: CancellationToken) -> Result<Vec<AudioCandidate>, P2pError> {
        self.ensure_connected(&cancel).await?;
        let text = format!("{} flac", query.text());
        debug!(query = %text, "Soulseek search");
        let responses = self.client
            .search(text)
            .idle_timeout(SEARCH_IDLE)
            .maximum_duration(SEARCH_MAX)
            .response_limit(100)
            .file_limit(5000)
            .into_future();
        let mut responses = match cancellable(responses, &cancel).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(map_client_error("Soulseek: поиск не удался", e)),
            Err(e) => return Err(e),
        };
        let limit = query.limit.max(1);
        let mut candidates: Vec<AudioCandidate> = Vec::new();
        // Досрочное завершение: набрали `limit` кандидатов от РАЗНЫХ быстрых пиров
        // (free_slot + скорость) — даём SEARCH_GRACE на более быстрых и заканчиваем.
        // Мгновенного break по числу файлов одного пира нет: медленный пир с большой
        // папкой не обрывает поиск раньше времени.
        let mut good_peers: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut grace_deadline: Option<tokio::time::Instant> = None;
        loop {
            if cancel.is_cancelled() {
                return Err(P2pError::Cancelled);
            }
            // Уже набрали достаточно быстрых пиров — до конца grace, и выходим.
            let next = if let Some(deadline) = grace_deadline {
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                match tokio::time::timeout(left, responses.next()).await {
                    Ok(v) => v,
                    Err(_) => break, // grace истёк
                }
            } else {
                match cancellable(responses.next(), &cancel).await {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                }
            };
            let Some(response) = next else { break };
            let mut matched = 0;
            for file in &response.files {
                if let Some(candidate) = Self::candidate(&response, file) {
                    if matches_query(&candidate, query) {
                        candidates.push(candidate);
                        matched += 1;
                    }
                }
            }
            if response.has_free_upload_slot && response.upload_speed > GOOD_SPEED && matched > 0 {
                good_peers.insert(response.username.clone());
                if good_peers.len() >= limit {
                    grace_deadline.get_or_insert_with(|| tokio::time::Instant::now() + SEARCH_GRACE);
                }
            }
        }
        candidates.sort_by(|a, b| compare_candidates(a, b, query));
        candidates.truncate(limit);
        info!(query = %query.text(), found = candidates.len(), peers = good_peers.len(), "Soulseek FLAC search finished");
        Ok(candidates)
    }

    async fn resolve(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioCandidate, P2pError> {
        let SourceLocator::Soulseek { username, remote_filename, size } = locator else {
            return Err(P2pError::InvalidInput("не Soulseek-источник".into()));
        };
        self.ensure_connected(&cancel).await?;
        Ok(AudioCandidate {
            id: format!("slsk:{username}:{remote_filename}:{size}"),
            provider: ProviderKind::Soulseek,
            title: remote_filename.rsplit(['\\', '/']).next().unwrap_or(remote_filename).to_string(),
            remote_path: remote_filename.clone(),
            size: *size,
            duration: None,
            bit_rate: None,
            sample_rate: None,
            bit_depth: None,
            free_slot: true,
            queue_length: None,
            upload_speed: None,
            locator: locator.clone(),
        })
    }

    async fn download(&self, request: DownloadRequest) -> Result<DownloadedAudio, P2pError> {
        let SourceLocator::Soulseek { username, remote_filename, size } = &request.locator else {
            return Err(P2pError::InvalidInput("не Soulseek-источник".into()));
        };
        // Скачиваем в кэш-файл этого кандидата (username+filename+size), а не прямо
        // в destination: упавший кандидат не оставляет чужих байтов — прошлый баг с
        // resume ломал FLAC, когда второй кандидат писал поверх недокачанного первого.
        // Атомарно переносим только проверенный по размеру и сигнатуре файл.
        let cache = self.cache_path(username, remote_filename, *size);
        let staged = DownloadRequest { destination: cache.clone(), ..request.clone() };
        self.download_file(username, remote_filename, *size, &staged, true).await?;
        let actual = tokio::fs::metadata(&cache).await?.len();
        if actual != *size {
            let _ = tokio::fs::remove_file(&cache).await;
            return Err(P2pError::Unavailable(format!("Soulseek: получено {actual} из {size} байт")));
        }
        // Не просто расширение: FLAC должен начинаться с сигнатуры fLaC.
        let mut file = tokio::fs::File::open(&cache).await?;
        let mut signature = [0; 4];
        tokio::io::AsyncReadExt::read_exact(&mut file, &mut signature).await?;
        if &signature != b"fLaC" {
            let _ = tokio::fs::remove_file(&cache).await;
            return Err(P2pError::Unavailable("Soulseek: файл имеет расширение .flac, но не FLAC-сигнатуру".into()));
        }
        // Проверено: переносим на место (rename, при неудаче — копирование).
        let destination = &request.destination;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::rename(&cache, destination).await.is_err() {
            tokio::fs::copy(&cache, destination).await?;
            let _ = tokio::fs::remove_file(&cache).await;
        }
        Ok(DownloadedAudio {
            path: destination.clone(),
            provider: ProviderKind::Soulseek,
            source_description: format!("Soulseek / {username} / {remote_filename}"),
        })
    }

    async fn stream(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioStream, P2pError> {
        self.open_cached_stream(locator, cancel).await
    }

    async fn shutdown(&self) -> Result<(), P2pError> {
        self.client.shutdown().await;
        Ok(())
    }
}

fn matches_query(candidate: &AudioCandidate, query: &SearchQuery) -> bool {
    let path = candidate.remote_path.to_lowercase();
    // Слова запроса без скобок и пунктуации: «Inference (I)» → «inference», «I»,
    // чтобы трек находился и у пира без скобок в названии.
    let clean = |word: &str| word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let required = query.artist.split_whitespace().chain(query.title.split_whitespace());
    if !required.filter(|w| w.chars().count() > 1).all(|word| path.contains(&clean(word))) {
        return false;
    }
    match (query.duration, candidate.duration) {
        (Some(want), Some(got)) => {
            let tolerance = (want.as_secs_f64() * 0.08).max(8.0);
            (want.as_secs_f64() - got.as_secs_f64()).abs() <= tolerance
        }
        _ => true,
    }
}

fn compare_candidates(a: &AudioCandidate, b: &AudioCandidate, query: &SearchQuery) -> Ordering {
    let score = |c: &AudioCandidate| {
        let mut score = 0i64;
        if c.free_slot { score += 100_000_000; }
        // Скорость — главный фактор после free_slot: медленный пир тянет очередь.
        // ×500: пир на 5 МБ/с обходит пира на 500 КБ/с даже с 24-bit файлом.
        score += i64::from(c.upload_speed.unwrap_or(0).min(50_000_000) / 1024) * 500;
        if c.bit_depth == Some(24) { score += 50_000; }
        score += i64::from(c.sample_rate.unwrap_or(0).min(192_000));
        // Занятая очередь и нулевая скорость — сильный штраф.
        if c.queue_length.unwrap_or(0) > 0 {
            score -= i64::from(c.queue_length.unwrap_or(0).min(100_000)) * 10_000;
        }
        if c.upload_speed.unwrap_or(1) == 0 { score -= 50_000_000; }
        if let (Some(want), Some(got)) = (query.duration, c.duration) {
            score -= (want.as_secs_f64() - got.as_secs_f64()).abs() as i64 * 1000;
        }
        score
    };
    score(b).cmp(&score(a)).then_with(|| a.remote_path.cmp(&b.remote_path))
}

fn map_client_error(context: &str, error: soulseek_client::Error) -> P2pError {
    use soulseek_client::ErrorKind;
    let text = format!("{context}: {error}");
    if error.is_canceled() {
        P2pError::Cancelled
    } else if error.is_timeout() {
        P2pError::Timeout(text)
    } else if matches!(error.kind(), ErrorKind::Connection(_) | ErrorKind::Address | ErrorKind::Io) {
        P2pError::Network(text)
    } else {
        P2pError::Unavailable(text)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_filter_is_tolerant() {
        let c = AudioCandidate {
            id: "x".into(), provider: ProviderKind::Soulseek, title: "Song.flac".into(),
            remote_path: "Artist\\Album\\Song.flac".into(), size: 100,
            duration: Some(Duration::from_secs(190)), bit_rate: None, sample_rate: None, bit_depth: None,
            free_slot: true, queue_length: None, upload_speed: None,
            locator: SourceLocator::Soulseek { username: "u".into(), remote_filename: "f".into(), size: 100 },
        };
        let q = SearchQuery { artist: "Artist".into(), title: "Song".into(), album: None, duration: Some(Duration::from_secs(200)), limit: 4 };
        assert!(matches_query(&c, &q));
    }

    #[test]
    fn parens_and_punctuation_do_not_break_matching() {
        // Пир без скобок в названии: «Inference I» должен подходить под «Inference (I)».
        let c = AudioCandidate {
            id: "x".into(), provider: ProviderKind::Soulseek, title: "Inference I".into(),
            remote_path: "Pceengra\\2021\\07 Inference I.flac".into(), size: 100,
            duration: None, bit_rate: None, sample_rate: None, bit_depth: None,
            free_slot: true, queue_length: None, upload_speed: None,
            locator: SourceLocator::Soulseek { username: "u".into(), remote_filename: "f".into(), size: 100 },
        };
        let q = SearchQuery { artist: "Pceengra".into(), title: "Inference (I)".into(), album: None, duration: None, limit: 4 };
        assert!(matches_query(&c, &q));
    }

    #[test]
    fn fast_peer_beats_slow_24bit_and_queue() {
        let base = |speed: u32, free: bool, queue: u32, depth: u32| AudioCandidate {
            id: "x".into(), provider: ProviderKind::Soulseek, title: "t".into(),
            remote_path: "a\\t.flac".into(), size: 100, duration: None,
            bit_rate: None, sample_rate: Some(44_100), bit_depth: (depth != 0).then_some(depth),
            free_slot: free, queue_length: Some(queue), upload_speed: Some(speed),
            locator: SourceLocator::Soulseek { username: "u".into(), remote_filename: "t.flac".into(), size: 100 },
        };
        let q = SearchQuery { artist: "a".into(), title: "t".into(), album: None, duration: None, limit: 4 };
        // Быстрый 16-bit пир обгоняет медленный 24-bit: скорость важнее битности.
        let fast = base(5_000_000, true, 0, 16);
        let slow24 = base(200_000, true, 0, 24);
        assert_eq!(compare_candidates(&slow24, &fast, &q), Ordering::Greater);
        // Занятая очередь сильно штрафует даже быстрый пир.
        let queued = base(5_000_000, true, 30, 24);
        assert_eq!(compare_candidates(&queued, &fast, &q), Ordering::Greater);
    }
}
