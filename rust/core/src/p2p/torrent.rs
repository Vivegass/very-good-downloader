//! BitTorrent через librqbit: magnet → метаданные → выбор FLAC → файл/поток.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ListenerOptions, Session, SessionOptions,
};
use librqbit::ManagedTorrent;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use super::{
    cancellable, AudioCandidate, AudioSourceProvider, AudioStream, DownloadRequest,
    DownloadedAudio, P2pError, ProviderKind, SearchQuery, SourceLocator,
};

type ManagedTorrentHandle = Arc<ManagedTorrent>;

pub struct TorrentProvider {
    session: Arc<Session>,
    output: PathBuf,
    add_lock: Mutex<()>,
}

#[derive(Clone, Debug)]
pub struct TorrentFile {
    pub index: usize,
    pub path: PathBuf,
    pub size: u64,
    pub absolute_path: PathBuf,
}

impl TorrentProvider {
    pub async fn new(output: PathBuf) -> Result<Self, P2pError> {
        tokio::fs::create_dir_all(&output).await?;
        let opts = SessionOptions {
            // P2P-движок приложения только скачивает: не раздаёт внешнюю музыку.
            disable_upload: true,
            listen: Some(ListenerOptions {
                listen_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                enable_upnp_port_forwarding: false,
                ..Default::default()
            }),
            disable_local_service_discovery: true,
            client_name_and_version: Some(format!("Very Good Downloader/{}", crate::appinfo::APP_VERSION)),
            ..Default::default()
        };
        let session = Session::new_with_opts(output.clone(), opts)
            .await
            .map_err(|e| P2pError::Unavailable(format!("librqbit: не запустилась сессия: {e:#}")))?;
        Ok(Self { session, output, add_lock: Mutex::new(()) })
    }

    /// Добавить раздачу. При `paused` — только получить метаданные, не качая:
    /// для выборки файлов до старта (дискография не задевает диск соседними
    /// альбомами). Качающий вызов потом снимет паузу через [`Self::choose_only`]
    /// или заново стартует раздачу целиком.
    async fn add(&self, magnet: &str, cancel: &CancellationToken, paused: bool) -> Result<ManagedTorrentHandle, P2pError> {
        // add_torrent ждёт метаданные magnet. Serialize добавление, чтобы выбор
        // файлов не гонялся с уже существующим handle другого запроса.
        let _guard = self.add_lock.lock().await;
        let options = AddTorrentOptions {
            output_folder: Some(self.output.to_string_lossy().into_owned()),
            overwrite: true,
            paused,
            ..Default::default()
        };
        let response = self.session.add_torrent(AddTorrent::from_url(magnet.to_string()), Some(options));
        let response = match cancellable(response, cancel).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(P2pError::Network(format!("librqbit: magnet не разрешился: {e:#}"))),
            Err(e) => return Err(e),
        };
        let handle = match response {
            AddTorrentResponse::Added(_, h) | AddTorrentResponse::AlreadyManaged(_, h) => h,
            AddTorrentResponse::ListOnly(_) => return Err(P2pError::Other("librqbit: неожиданный list-only ответ".into())),
        };
        let initialized = handle.wait_until_initialized();
        match cancellable(initialized, cancel).await {
            Ok(Ok(())) => Ok(handle),
            Ok(Err(e)) => Err(P2pError::Unavailable(format!("librqbit: инициализация раздачи: {e:#}"))),
            Err(e) => Err(e),
        }
    }

    pub fn files(handle: &ManagedTorrentHandle) -> Result<Vec<TorrentFile>, P2pError> {
        let metadata = handle.metadata.load_full().ok_or_else(|| P2pError::NotFound("метаданные раздачи ещё не получены".into()))?;
        let output = handle.output_folder();
        Ok(metadata.file_infos.iter().enumerate().map(|(index, f)| TorrentFile {
            index,
            path: f.relative_filename.clone(),
            size: f.len,
            absolute_path: output.join(&f.relative_filename),
        }).collect())
    }

    pub fn flac_files(handle: &ManagedTorrentHandle) -> Result<Vec<TorrentFile>, P2pError> {
        let files = Self::files(handle)?
            .into_iter()
            .filter(|f| f.path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("flac")) && f.size > 0)
            .collect::<Vec<_>>();
        if files.is_empty() {
            Err(P2pError::NotFound("в раздаче нет FLAC-файлов".into()))
        } else {
            Ok(files)
        }
    }

    pub async fn select_flac(&self, locator: &SourceLocator, cancel: &CancellationToken) -> Result<(ManagedTorrentHandle, TorrentFile), P2pError> {
        let SourceLocator::Torrent { magnet, file_index } = locator else {
            return Err(P2pError::InvalidInput("не BitTorrent-источник".into()));
        };
        // На паузе: метаданные без скачивания, файл выберется до старта.
        let handle = self.add(magnet, cancel, true).await?;
        let files = Self::flac_files(&handle)?;
        let file = files.iter().find(|f| f.index == *file_index).cloned().ok_or_else(|| {
            let available = files.iter().map(|f| f.index.to_string()).collect::<Vec<_>>().join(", ");
            P2pError::NotFound(format!("файл {file_index} не FLAC или не существует; FLAC-индексы: {available}"))
        })?;
        Ok((handle, file))
    }

    /// Все FLAC-файлы раздачи (пусто — если FLAC там нет). Порядок как в метаданных.
    /// Раздача добавляется на паузе: метаданные без скачивания, выбор файлов — до старта.
    pub async fn flac_list(&self, magnet: &str, cancel: &CancellationToken) -> Result<Vec<TorrentFile>, P2pError> {
        let handle = self.add(magnet, cancel, true).await?;
        Ok(Self::files(&handle)?
            .into_iter()
            .filter(|f| f.path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("flac")) && f.size > 0)
            .collect())
    }

    async fn choose_only(&self, handle: &ManagedTorrentHandle, index: usize, cancel: &CancellationToken) -> Result<(), P2pError> {
        if handle.only_files().as_deref() == Some(&[index]) {
            return Ok(());
        }
        let wanted = HashSet::from([index]);
        let update = self.session.update_only_files(handle, &wanted);
        match cancellable(update, cancel).await {
            Ok(Ok(())) => {
                if handle.is_paused() {
                    match cancellable(self.session.unpause(handle), cancel).await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => return Err(P2pError::Unavailable(format!("librqbit: продолжение раздачи: {e:#}"))),
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
            Ok(Err(e)) => Err(P2pError::Unavailable(format!("librqbit: выбор файла: {e:#}"))),
            Err(e) => Err(e),
        }
    }

    async fn candidate(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioCandidate, P2pError> {
        let (handle, file) = self.select_flac(locator, &cancel).await?;
        Ok(AudioCandidate {
            id: format!("bt:{:?}:{}", handle.info_hash(), file.index),
            provider: ProviderKind::BitTorrent,
            title: file.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "track.flac".into()),
            remote_path: file.path.to_string_lossy().into_owned(),
            size: file.size,
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

    async fn download_selected(&self, request: &DownloadRequest) -> Result<(ManagedTorrentHandle, TorrentFile), P2pError> {
        let (handle, file) = self.select_flac(&request.locator, &request.cancel).await?;
        self.choose_only(&handle, file.index, &request.cancel).await?;
        if let Some(parent) = request.destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&request.destination)
            .await?;
        let have = output.metadata().await?.len().min(file.size);
        if have < file.size {
            let mut input = handle.clone().stream(file.index)
                .await
                .map_err(|e| P2pError::Unavailable(format!("librqbit: открыть поток: {e:#}")))?;
            input.seek(std::io::SeekFrom::Start(have)).await?;
            let mut left = file.size - have;
            let mut buffer = vec![0u8; 256 * 1024];
            let mut last_report = std::time::Instant::now();
            while left > 0 {
                let wanted = left.min(buffer.len() as u64) as usize;
                let read = match cancellable(input.read(&mut buffer[..wanted]), &request.cancel).await {
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(P2pError::Network(format!("librqbit: чтение потока: {e}"))),
                    Err(e) => return Err(e),
                };
                if read == 0 {
                    return Err(P2pError::Unavailable(format!("librqbit: поток закончился раньше времени, осталось {left} байт")));
                }
                output.write_all(&buffer[..read]).await?;
                left -= read as u64;
                if let Some(cb) = &request.progress {
                    if last_report.elapsed() >= Duration::from_millis(250) || left == 0 {
                        let stats = handle.stats();
                        cb(super::TransferProgress {
                            downloaded: file.size - left,
                            total: file.size,
                            bytes_per_second: stats.live.as_ref().map(|l| l.download_speed.as_bytes()),
                            message: format!("BitTorrent: {}/{} файлов выбрано", stats.file_progress.len(), handle.metadata.load().as_ref().map(|m| m.file_infos.len()).unwrap_or(0)),
                        });
                        last_report = std::time::Instant::now();
                    }
                }
            }
            output.flush().await?;
        }
        let copied = request.destination.clone();
        // Теги/обложку добавит общий music_download, поэтому результат — временная
        // копия в назначенной пользователю папке, а torrent-cache остаётся для сидирования
        // последующих Range-запросов (upload в этой сборке отключён).
        drop(output);
        debug!(path = %copied.display(), "selected FLAC downloaded");
        Ok((handle, file))
    }
}

#[async_trait]
impl AudioSourceProvider for TorrentProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::BitTorrent
    }

    async fn search(&self, _query: &SearchQuery, _cancel: CancellationToken) -> Result<Vec<AudioCandidate>, P2pError> {
        Err(P2pError::Unavailable("BitTorrent не имеет встроенного поиска по названию: нужна magnet-ссылка".into()))
    }

    async fn resolve(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioCandidate, P2pError> {
        self.candidate(locator, cancel).await
    }

    async fn download(&self, request: DownloadRequest) -> Result<DownloadedAudio, P2pError> {
        let (_, file) = self.download_selected(&request).await?;
        let actual = tokio::fs::metadata(&request.destination).await?.len();
        if actual != file.size {
            return Err(P2pError::Unavailable(format!("librqbit: получено {actual} из {} байт", file.size)));
        }
        Ok(DownloadedAudio {
            path: request.destination.clone(),
            provider: ProviderKind::BitTorrent,
            source_description: format!("BitTorrent / {}", file.path.display()),
        })
    }

    async fn stream(&self, locator: &SourceLocator, cancel: CancellationToken) -> Result<AudioStream, P2pError> {
        let (handle, file) = self.select_flac(locator, &cancel).await?;
        self.choose_only(&handle, file.index, &cancel).await?;
        // Для HTTP используется уже заполненный torrent-файл; stream.rs может
        // читать его на любом offset, а librqbit заполняет недостающие куски.
        // Файл в cache не трогаем: stream отдаёт handle-backed логический файл.
        let server = serve_torrent_stream(handle, file.clone(), cancel).await?;
        let url = server.url();
        Ok(AudioStream {
            url,
            size: file.size,
            mime: "audio/flac",
            provider: ProviderKind::BitTorrent,
            title: file.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "track.flac".into()),
            server,
        })
    }

    async fn shutdown(&self) -> Result<(), P2pError> {
        self.session.stop().await;
        Ok(())
    }
}

/// Поток в torrent-cache. Здесь файл открывается синхронно по смещению только
/// после получения куска от librqbit FileStream, поэтому sparse-данные не отдаются.
async fn serve_torrent_stream(handle: ManagedTorrentHandle, file: TorrentFile, cancel: CancellationToken) -> Result<Arc<super::StreamServer>, P2pError> {
    let reader = handle.clone().stream(file.index)
        .await
        .map_err(|e| P2pError::Unavailable(format!("librqbit: поток файла: {e:#}")))?;
    super::serve_async_stream(reader, file.size, "audio/flac", file.path.to_string_lossy().into_owned(), ProviderKind::BitTorrent, cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn output_join_keeps_relative_torrent_path() {
        let root = PathBuf::from("cache");
        let file = Path::new("Album/01 track.flac");
        assert_eq!(root.join(file), PathBuf::from("cache/Album/01 track.flac"));
    }
}
