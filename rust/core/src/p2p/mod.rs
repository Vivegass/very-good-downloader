//! Гибридный P2P-движок FLAC: общий async API, Soulseek и BitTorrent.
//!
//! Синхронная очередь приложения пользуется [`engine`], не создавая Tokio runtime
//! на каждый трек. Плеер получает локальные HTTP Range URL из [`AudioStream::url`].

mod soulseek;
mod stream;
mod torrent;
mod trackers;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::settings::Settings;

pub use soulseek::SoulseekProvider;
pub use stream::{serve_async_stream, serve_stream, StreamServer};
pub use torrent::{TorrentFile, TorrentProvider};
pub use trackers::{pick_track_file, search_trackers, TrackerHit};

#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("неверный запрос: {0}")]
    InvalidInput(String),
    #[error("источник не настроен: {0}")]
    Configuration(String),
    #[error("сеть: {0}")]
    Network(String),
    #[error("P2P-источник недоступен: {0}")]
    Unavailable(String),
    #[error("файл не найден: {0}")]
    NotFound(String),
    #[error("операция отменена")]
    Cancelled,
    #[error("превышено время ожидания: {0}")]
    Timeout(String),
    #[error("локальный файл: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl P2pError {
    pub fn network(error: impl std::fmt::Display) -> Self {
        Self::Network(error.to_string())
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Soulseek,
    BitTorrent,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Soulseek => f.write_str("Soulseek"),
            Self::BitTorrent => f.write_str("BitTorrent"),
        }
    }
}

/// Данные, необходимые для адресации файла без зависимости от протокола.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum SourceLocator {
    Soulseek {
        username: String,
        remote_filename: String,
        size: u64,
    },
    Torrent {
        magnet: String,
        file_index: usize,
    },
}

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration: Option<Duration>,
    pub limit: usize,
}

impl SearchQuery {
    pub fn text(&self) -> String {
        format!("{} {}", self.artist.trim(), self.title.trim()).trim().to_string()
    }
}

#[derive(Clone, Debug)]
pub struct AudioCandidate {
    pub id: String,
    pub provider: ProviderKind,
    pub title: String,
    pub remote_path: String,
    pub size: u64,
    pub duration: Option<Duration>,
    pub bit_rate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub free_slot: bool,
    pub queue_length: Option<u32>,
    pub upload_speed: Option<u32>,
    pub locator: SourceLocator,
}

#[derive(Clone, Debug, Default)]
pub struct TransferProgress {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_second: Option<u64>,
    pub message: String,
}

pub type ProgressCallback = Arc<dyn Fn(TransferProgress) + Send + Sync>;

#[derive(Clone)]
pub struct DownloadRequest {
    pub locator: SourceLocator,
    pub destination: PathBuf,
    pub progress: Option<ProgressCallback>,
    pub cancel: CancellationToken,
}

#[derive(Clone, Debug)]
pub struct DownloadedAudio {
    pub path: PathBuf,
    pub provider: ProviderKind,
    pub source_description: String,
}

#[derive(Clone)]
pub struct AudioStream {
    /// URL только на loopback-интерфейсе. Поддерживает GET, HEAD и один Range.
    pub url: String,
    pub size: u64,
    pub mime: &'static str,
    pub provider: ProviderKind,
    pub title: String,
    server: Arc<StreamServer>,
}

impl AudioStream {
    pub async fn close(&self) {
        self.server.shutdown().await;
    }
}

/// Единый протокол-независимый интерфейс. Реализации не делятся деталями
/// `soulseek-client` или `librqbit` с музыкальным загрузчиком приложения.
#[async_trait]
pub trait AudioSourceProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    async fn search(&self, query: &SearchQuery, cancel: CancellationToken) -> Result<Vec<AudioCandidate>, P2pError>;

    async fn resolve(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioCandidate, P2pError>;

    async fn download(&self, request: DownloadRequest) -> Result<DownloadedAudio, P2pError>;

    async fn stream(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioStream, P2pError>;

    async fn shutdown(&self) -> Result<(), P2pError>;
}

pub struct HybridEngine {
    soulseek: Option<Arc<SoulseekProvider>>,
    torrent: Arc<TorrentProvider>,
}

impl HybridEngine {
    /// Soulseek не обязателен: без логина/пароля движок работает только с magnet.
    pub async fn new(settings: &Settings) -> Result<Self, P2pError> {
        let cache = crate::appinfo::data_dir().join("p2p");
        tokio::fs::create_dir_all(&cache).await?;
        let soulseek = match SoulseekProvider::new(settings, cache.join("soulseek")).await {
            Ok(provider) => {
                info!("Soulseek provider enabled");
                Some(Arc::new(provider))
            }
            Err(P2pError::Configuration(message)) => {
                info!(reason = %message, "Soulseek disabled; engine is magnet-only");
                None
            }
            Err(e) => return Err(e),
        };
        let torrent = Arc::new(TorrentProvider::new(cache.join("torrent")).await?);
        Ok(Self { soulseek, torrent })
    }

    pub fn soulseek(&self) -> Option<Arc<dyn AudioSourceProvider>> {
        self.soulseek.clone().map(|p| p as Arc<dyn AudioSourceProvider>)
    }

    pub fn torrent(&self) -> Arc<dyn AudioSourceProvider> {
        self.torrent.clone() as Arc<dyn AudioSourceProvider>
    }

    fn provider(&self, kind: &ProviderKind) -> Result<Arc<dyn AudioSourceProvider>, P2pError> {
        match kind {
            ProviderKind::Soulseek => self.soulseek.clone().map(|p| p as Arc<dyn AudioSourceProvider>).ok_or_else(|| {
                P2pError::Configuration("Soulseek не настроен: укажите логин и пароль в настройках музыки".into())
            }),
            ProviderKind::BitTorrent => Ok(self.torrent.clone() as Arc<dyn AudioSourceProvider>),
        }
    }

    /// Magnet/файл из раздачи выбирается напрямую; текстовый поиск идёт в Soulseek.
    pub async fn resolve(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioCandidate, P2pError> {
        self.provider(&provider_of(locator))?.resolve(locator, cancel).await
    }

    /// BitTorrent не имеет поиска по названию: это осознанное ограничение,
    /// а не скрытый вызов внешнего torrent-index.
    pub async fn search(&self, query: &SearchQuery, cancel: CancellationToken) -> Result<Vec<AudioCandidate>, P2pError> {
        match &self.soulseek {
            Some(soulseek) => soulseek.search(query, cancel).await,
            None => Err(P2pError::Configuration("Soulseek не настроен: текстовый поиск FLAC недоступен".into())),
        }
    }

    pub async fn download(&self, request: DownloadRequest) -> Result<DownloadedAudio, P2pError> {
        self.provider(&provider_of(&request.locator))?.download(request).await
    }

    pub async fn stream(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioStream, P2pError> {
        self.provider(&provider_of(locator))?.stream(locator, cancel).await
    }

    /// Все FLAC-файлы magnet-раздачи (для музыкального загрузчика).
    pub async fn torrent_files(&self, magnet: &str, cancel: CancellationToken) -> Result<Vec<TorrentFile>, P2pError> {
        self.torrent.flac_list(magnet, &cancel).await
    }

    pub async fn shutdown(&self) {
        if let Some(soulseek) = &self.soulseek {
            if let Err(e) = soulseek.shutdown().await {
                warn!(error = %e, "Soulseek shutdown failed");
            }
        }
        if let Err(e) = self.torrent.shutdown().await {
            warn!(error = %e, "BitTorrent shutdown failed");
        }
    }
}

pub fn provider_of(locator: &SourceLocator) -> ProviderKind {
    match locator {
        SourceLocator::Soulseek { .. } => ProviderKind::Soulseek,
        SourceLocator::Torrent { .. } => ProviderKind::BitTorrent,
    }
}

pub fn is_magnet(value: &str) -> bool {
    value.trim_start().get(..7).is_some_and(|s| s.eq_ignore_ascii_case("magnet:"))
}

/// Ссылка выбранного файла раздачи: `magnet:…#file=<нулевой индекс>` либо
/// `torrent:<url-encoded magnet>#file=<индекс>`. Индекс — именно из списка файлов
/// метаданных librqbit, не номер трека альбома.
pub fn torrent_locator(input: &str) -> Result<SourceLocator, P2pError> {
    let input = input.trim();
    let (base, file) = input.split_once("#file=").map_or((input, None), |(a, b)| (a, Some(b)));
    let index = match file {
        Some(v) => Some(v.parse::<usize>().map_err(|_| P2pError::InvalidInput("индекс файла раздачи должен быть числом".into()))?),
        None => None,
    };
    let magnet = if let Some(encoded) = base.strip_prefix("torrent:") {
        percent_decode(encoded)?
    } else {
        base.to_string()
    };
    if !is_magnet(&magnet) {
        return Err(P2pError::InvalidInput("ожидалась magnet-ссылка".into()));
    }
    let file_index = index.or_else(|| magnet_param(&magnet, "so").and_then(|v| v.parse().ok())).unwrap_or(0);
    Ok(SourceLocator::Torrent { magnet, file_index })
}

pub fn locator_url(locator: &SourceLocator) -> String {
    match locator {
        SourceLocator::Soulseek { username, remote_filename, size } => {
            format!("slsk:{}#{}#{}", percent_encode(username), percent_encode(remote_filename), size)
        }
        SourceLocator::Torrent { magnet, file_index } => format!("torrent:{}#file={file_index}", percent_encode(magnet)),
    }
}

/// Жёсткий таймаут на получение метаданных раздачи при поиске трека:
/// мёртвый торрент не должен стопорить очередь загрузки.
const TRACKER_META_TIMEOUT: Duration = Duration::from_secs(20);

/// Для музыкального загрузчика: Soulseek — точное совпадение трека.
pub async fn fetch_soulseek(query: &SearchQuery, destination: &Path, progress: Option<ProgressCallback>, cancel: CancellationToken) -> Result<DownloadedAudio, P2pError> {
    let engine = engine().await?;
    let mut last_error = P2pError::NotFound(format!("FLAC «{}» не найден", query.text()));
    if let Some(soulseek) = engine.soulseek() {
        match soulseek.search(query, cancel.clone()).await {
            Ok(candidates) => {
                for candidate in candidates {
                    let request = DownloadRequest {
                        locator: candidate.locator.clone(),
                        destination: destination.to_path_buf(),
                        progress: progress.clone(),
                        cancel: cancel.clone(),
                    };
                    match engine.download(request).await {
                        Ok(done) => return Ok(done),
                        Err(P2pError::Cancelled) => return Err(P2pError::Cancelled),
                        Err(e) => {
                            warn!(candidate = %candidate.remote_path, error = %e, "Soulseek candidate failed");
                            last_error = e;
                        }
                    }
                }
            }
            Err(e) => warn!(error = %e, "Soulseek search failed"),
        }
    }
    Err(last_error)
}

/// Для музыкального загрузчика: торрент-трекеры (apibay) — раздачи
/// «Artist - Album», файл трека выбирается по названию. Метаданные каждой
/// раздачи — с жёстким таймаутом [`TRACKER_META_TIMEOUT`], чтобы мёртвый
/// торрент не стопорил очередь.
pub async fn fetch_tracker_track(query: &SearchQuery, destination: &Path, progress: Option<ProgressCallback>, cancel: CancellationToken) -> Result<DownloadedAudio, P2pError> {
    let engine = engine().await?;
    let text = query.text();
    if text.is_empty() {
        return Err(P2pError::NotFound("пустой запрос".into()));
    }
    let hits = trackers::search_trackers(&text, TRACKER_META_TIMEOUT)?;
    let mut last_error = P2pError::NotFound(format!("FLAC «{text}» на трекерах не найден"));
    for hit in hits.iter().take(3) {
        if cancel.is_cancelled() {
            return Err(P2pError::Cancelled);
        }
        tracing::debug!(torrent = %hit.name, seeders = hit.seeders, "tracker candidate");
        // Метаданные с таймаутом: мёртвая раздача пропускается, очередь не ждёт.
        let meta = tokio::time::timeout(
            TRACKER_META_TIMEOUT,
            engine.torrent_files(&hit.magnet, cancel.clone()),
        );
        let files = match meta.await {
            Ok(Ok(f)) => f,
            Ok(Err(P2pError::Cancelled)) => return Err(P2pError::Cancelled),
            Ok(Err(e)) => {
                warn!(torrent = %hit.name, error = %e, "tracker metadata failed");
                continue;
            }
            Err(_) => {
                warn!(torrent = %hit.name, timeout = ?TRACKER_META_TIMEOUT, "tracker metadata timed out");
                continue;
            }
        };
        let Some(index) = pick_track_file(query, &files) else {
            continue;
        };
        let locator = SourceLocator::Torrent { magnet: hit.magnet.clone(), file_index: index };
        let request = DownloadRequest {
            locator,
            destination: destination.to_path_buf(),
            progress: progress.clone(),
            cancel: cancel.clone(),
        };
        match engine.download(request).await {
            Ok(done) => return Ok(done),
            Err(P2pError::Cancelled) => return Err(P2pError::Cancelled),
            Err(e) => {
                warn!(torrent = %hit.name, error = %e, "tracker candidate failed");
                last_error = e;
            }
        }
    }
    Err(last_error)
}

static ENGINE: OnceLock<Mutex<Option<Arc<HybridEngine>>>> = OnceLock::new();
static LOGGER: OnceLock<()> = OnceLock::new();
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Общий multi-thread runtime процесса. Синхронная очередь приложения вызывает
/// P2P через [`block_on`], не создавая свой runtime на каждый трек.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("запустить Tokio runtime для P2P")
    })
}

/// Синхронный вызов асинхронного P2P-API из потоков очереди (std::thread).
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    runtime().block_on(future)
}

fn init_logging() {
    LOGGER.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_env("VGD_P2P_LOG")
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("vgd_core::p2p=info,soulseek_client=warn,librqbit=warn"));
        if let Err(e) = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init() {
            eprintln!("P2P logger: {e}");
        }
    });
}

/// Общий runtime и сессии на процесс. При смене логина Soulseek в настройках
/// сессия создаётся заново, а старая закрывается.
pub async fn engine() -> Result<Arc<HybridEngine>, P2pError> {
    init_logging();
    let lock = ENGINE.get_or_init(|| Mutex::new(None));
    let mut slot = lock.lock().await;
    if let Some(e) = slot.as_ref() {
        return Ok(e.clone());
    }
    let settings = Settings::load();
    let created = HybridEngine::new(&settings).await?;
    let created = Arc::new(created);
    *slot = Some(created.clone());
    info!("hybrid P2P engine initialized");
    Ok(created)
}

/// Вызов приложения при изменении настроек/выходе. Сам runtime остаётся общим.
pub async fn shutdown_engine() {
    if let Some(lock) = ENGINE.get() {
        let old = lock.lock().await.take();
        if let Some(e) = old {
            e.shutdown().await;
        }
    }
}

pub(crate) async fn cancellable<T>(future: impl std::future::Future<Output = T>, cancel: &CancellationToken) -> Result<T, P2pError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(P2pError::Cancelled),
        value = future => Ok(value),
    }
}

pub(crate) fn magnet_param(magnet: &str, key: &str) -> Option<String> {
    let query = magnet.split_once('?')?.1;
    url::form_urlencoded::parse(query.split('#').next().unwrap_or("").as_bytes())
        .find_map(|(k, v)| (k == key).then_some(v.into_owned()))
}

fn percent_encode(value: &str) -> String {
    // НЕ form-urlencoded: '+' в пути/фрагменте не декодируется обратно, поэтому
    // только %XX для всех небуквенно-цифровых символов (включая '#', '\\', ':').
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn percent_decode(value: &str) -> Result<String, P2pError> {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .map(|v| v.into_owned())
        .map_err(|_| P2pError::InvalidInput("ссылка источника не в UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_torrent_file_urls() {
        let l = torrent_locator("magnet:?xt=urn:btih:0123456789012345678901234567890123456789&so=3").unwrap();
        assert!(matches!(l, SourceLocator::Torrent { file_index: 3, .. }));
        let l = torrent_locator("torrent:magnet%3A%3Fxt%3Durn%253Abtih%253Aabc%26dn%3DALBUM#file=2").unwrap();
        assert!(matches!(l, SourceLocator::Torrent { file_index: 2, .. }));
        assert!(matches!(provider_of(&l), ProviderKind::BitTorrent));
        assert!(torrent_locator("https://example.com/a").is_err());
    }

    #[test]
    fn soulseek_url_round_trip_components() {
        let user = "user name";
        let file = "@@share\\Альбом\\01 song.flac";
        let url = format!("slsk:{}#{}#42", percent_encode(user), percent_encode(file));
        let url = url::Url::parse(&url).unwrap();
        assert_eq!(url.path(), "user%20name");
        assert_eq!(percent_decode(url.path()).unwrap(), user);
        assert_eq!(percent_decode(url.fragment().unwrap().split('#').next().unwrap()).unwrap(), file);
    }
}
