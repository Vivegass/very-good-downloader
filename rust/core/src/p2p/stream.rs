//! Loopback HTTP/1.1: GET, HEAD, один байтовый Range; не читает FLAC целиком в память.
//!
//! Два backend-режима: resumable prefix (Soulseek) и AsyncRead+AsyncSeek (librqbit).

use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::header::{HeaderValue, ACCEPT_RANGES, CONNECTION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::{P2pError, ProviderKind};

const CHUNK: usize = 128 * 1024;
type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, std::io::Error>;
type BoxStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<Frame<Bytes>, std::io::Error>> + Send>>;

pub struct StreamServer {
    addr: std::net::SocketAddr,
    token: CancellationToken,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl StreamServer {
    pub fn url(&self) -> String {
        format!("http://{}/audio.flac", self.addr)
    }

    pub async fn shutdown(&self) {
        self.token.cancel();
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl Drop for StreamServer {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

#[derive(Clone)]
enum Backend {
    Prefix(PrefixState),
    Seekable(Arc<StdMutex<Option<Box<dyn AsyncReadSeek + Send + Unpin>>>>),
}

trait AsyncReadSeek: AsyncRead + AsyncSeek {}
impl<T: AsyncRead + AsyncSeek> AsyncReadSeek for T {}

#[derive(Clone)]
struct PrefixState {
    path: PathBuf,
    failed: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

#[derive(Clone)]
struct AppState {
    backend: Backend,
    len: u64,
    cancel: CancellationToken,
}

pub async fn serve_stream(
    path: PathBuf,
    len: u64,
    _mime: &'static str,
    _title: String,
    _provider: ProviderKind,
    cancel: CancellationToken,
) -> Result<Arc<StreamServer>, P2pError> {
    let failed = Arc::new(AtomicBool::new(false));
    let state = AppState {
        backend: Backend::Prefix(PrefixState { path, failed, notify: Arc::new(Notify::new()) }),
        len,
        cancel: cancel.child_token(),
    };
    bind(state).await
}

pub async fn serve_async_stream<R>(
    reader: R,
    len: u64,
    _mime: &'static str,
    _title: String,
    _provider: ProviderKind,
    cancel: CancellationToken,
) -> Result<Arc<StreamServer>, P2pError>
where
    R: AsyncRead + AsyncSeek + Send + Unpin + 'static,
{
    let state = AppState {
        backend: Backend::Seekable(Arc::new(StdMutex::new(Some(Box::new(reader))))),
        len,
        cancel: cancel.child_token(),
    };
    bind(state).await
}

async fn bind(state: AppState) -> Result<Arc<StreamServer>, P2pError> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let token = state.cancel.clone();
    let accept_cancel = token.clone();
    let task = tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                biased;
                _ = accept_cancel.cancelled() => break,
                accepted = listener.accept() => accepted,
            };
            let (socket, remote) = match accepted {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "P2P HTTP accept failed");
                    break;
                }
            };
            if !remote.ip().is_loopback() {
                continue;
            }
            let state = state.clone();
            tokio::spawn(async move {
                let cancel = state.cancel.clone();
                let service = service_fn(move |request| {
                    let state = state.clone();
                    async move { Ok::<_, Infallible>(handle(request, state).await) }
                });
                let io = TokioIo::new(socket);
                let connection = hyper::server::conn::http1::Builder::new().serve_connection(io, service);
                tokio::select! {
                    _ = cancel.cancelled() => {}
                    result = connection => {
                        if let Err(e) = result {
                            debug!(error = %e, "P2P HTTP connection ended with error");
                        }
                    }
                }
            });
        }
    });
    Ok(Arc::new(StreamServer { addr, token, task: Mutex::new(Some(task)) }))
}

async fn handle(request: Request<hyper::body::Incoming>, state: AppState) -> Response<BoxBody> {
    if request.uri().path() != "/audio.flac" {
        return simple(StatusCode::NOT_FOUND, "not found");
    }
    let method = request.method().clone();
    if method != hyper::Method::GET && method != hyper::Method::HEAD {
        return simple(StatusCode::METHOD_NOT_ALLOWED, "only GET and HEAD");
    }
    let range = match request.headers().get(RANGE) {
        Some(value) => match value.to_str().ok().and_then(|v| parse_range(v, state.len)) {
            Some(RangeParse::Valid(range)) => range,
            Some(RangeParse::Unsatisfiable) => return range_error(state.len),
            None => return simple(StatusCode::BAD_REQUEST, "invalid Range"),
        },
        None => ByteRange { start: 0, end: state.len.saturating_sub(1), len: state.len, partial: false },
    };
    let head = method == hyper::Method::HEAD;
    let status = if range.partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let content_range = format!("bytes {}-{}/{}", range.start, range.end, state.len);
    let stream = body_stream(state, range.clone(), head);
    let body = BodyExt::boxed_unsync(StreamBody::new(stream));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("audio/flac"));
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from(range.len));
    if range.partial {
        headers.insert(CONTENT_RANGE, HeaderValue::from_str(&content_range).unwrap_or(HeaderValue::from_static("bytes */0")));
    }
    response
}

fn body_stream(state: AppState, range: ByteRange, head: bool) -> BoxStream {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, std::io::Error>>(2);
    tokio::spawn(async move {
        if head || range.len == 0 {
            return;
        }
        let result = match state.backend {
            Backend::Prefix(prefix) => send_prefix(prefix, range, tx.clone(), state.cancel).await,
            Backend::Seekable(reader) => send_seekable(reader, range, tx.clone(), state.cancel).await,
        };
        if let Err(e) = result {
            if e.kind() != std::io::ErrorKind::BrokenPipe && e.kind() != std::io::ErrorKind::ConnectionReset {
                debug!(error = %e, "P2P HTTP stream read failed");
                let _ = tx.send(Err(e)).await;
            }
        }
    });
    Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
}

async fn send_prefix(
    state: PrefixState,
    range: ByteRange,
    tx: tokio::sync::mpsc::Sender<Result<Frame<Bytes>, std::io::Error>>,
    cancel: CancellationToken,
) -> std::io::Result<()> {
    let mut position = range.start;
    let end = range.end;
    let mut file = match tokio::fs::File::open(&state.path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tokio::fs::File::open(&state.path).await?
        }
        Err(e) => return Err(e),
    };
    file.seek(std::io::SeekFrom::Start(range.start)).await?;
    let mut buffer = vec![0u8; CHUNK];
    while position <= end {
        if cancel.is_cancelled() {
            return Ok(());
        }
        if state.failed.load(Ordering::Acquire) {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Soulseek download failed"));
        }
        let available = file.metadata().await?.len();
        if position >= available {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = state.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            continue;
        }
        let wanted = ((end - position + 1) as usize).min(CHUNK).min((available - position) as usize);
        let read = file.read(&mut buffer[..wanted]).await?;
        if read == 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        if tx.send(Ok(Frame::data(Bytes::copy_from_slice(&buffer[..read])))).await.is_err() {
            return Ok(());
        }
        position += read as u64;
    }
    Ok(())
}

async fn send_seekable(
    shared: Arc<StdMutex<Option<Box<dyn AsyncReadSeek + Send + Unpin>>>>,
    range: ByteRange,
    tx: tokio::sync::mpsc::Sender<Result<Frame<Bytes>, std::io::Error>>,
    cancel: CancellationToken,
) -> std::io::Result<()> {
    // Tokio mutex нужен, чтобы второй HTTP Range ждал первого, а не получал «busy».
    let reader = tokio::task::spawn_blocking({
        let shared = shared.clone();
        move || {
            let taken = shared.lock().map(|mut guard| guard.take());
            match taken {
                Ok(reader) => Ok(reader),
                Err(_) => Err(std::io::Error::other("torrent stream lock poisoned")),
            }
        }
    }).await.map_err(|e| std::io::Error::other(e.to_string()))??;
    let Some(mut reader) = reader else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "torrent stream is unavailable"));
    };
    let result = async {
        reader.seek(std::io::SeekFrom::Start(range.start)).await?;
        let mut left = range.len;
        let mut buffer = vec![0u8; CHUNK];
        while left > 0 {
            if cancel.is_cancelled() {
                return Ok(());
            }
            let wanted = left.min(CHUNK as u64) as usize;
            let read = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = reader.read(&mut buffer[..wanted]) => result?,
            };
            if read == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "torrent stream ended early"));
            }
            if tx.send(Ok(Frame::data(Bytes::copy_from_slice(&buffer[..read])))).await.is_err() {
                return Ok(());
            }
            left -= read as u64;
        }
        Ok(())
    }.await;
    tokio::task::spawn_blocking({
        let shared = shared.clone();
        move || {
            if let Ok(mut guard) = shared.lock() {
                *guard = Some(reader);
            }
        }
    }).await.map_err(|e| std::io::Error::other(e.to_string()))?;
    result
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
    len: u64,
    partial: bool,
}

enum RangeParse {
    Valid(ByteRange),
    Unsatisfiable,
}

fn parse_range(value: &str, total: u64) -> Option<RangeParse> {
    let value = value.trim();
    let spec = value.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    if total == 0 {
        return Some(RangeParse::Unsatisfiable);
    }
    if first.trim().is_empty() {
        let suffix = last.trim().parse::<u64>().ok()?;
        if suffix == 0 {
            return Some(RangeParse::Unsatisfiable);
        }
        let len = suffix.min(total);
        return Some(RangeParse::Valid(ByteRange { start: total - len, end: total - 1, len, partial: true }));
    }
    let start = first.trim().parse::<u64>().ok()?;
    if start >= total {
        return Some(RangeParse::Unsatisfiable);
    }
    let end = if last.trim().is_empty() { total - 1 } else { last.trim().parse::<u64>().ok()?.min(total - 1) };
    if end < start {
        return None;
    }
    Some(RangeParse::Valid(ByteRange { start, end, len: end - start + 1, partial: true }))
}

fn simple(status: StatusCode, text: &'static str) -> Response<BoxBody> {
    let body = Full::new(Bytes::from_static(text.as_bytes())).map_err(|e| match e {}).boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(text.len()));
    response.headers_mut().insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

fn range_error(total: u64) -> Response<BoxBody> {
    let mut response = simple(StatusCode::RANGE_NOT_SATISFIABLE, "range not satisfiable");
    if let Ok(v) = HeaderValue::from_str(&format!("bytes */{total}")) {
        response.headers_mut().insert(CONTENT_RANGE, v);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(value: &str, total: u64) -> ByteRange {
        match parse_range(value, total) {
            Some(RangeParse::Valid(r)) => r,
            _ => panic!("not valid: {value}"),
        }
    }

    #[test]
    fn parses_http_ranges() {
        assert_eq!(valid("bytes=2-4", 10), ByteRange { start: 2, end: 4, len: 3, partial: true });
        assert_eq!(valid("bytes=8-", 10), ByteRange { start: 8, end: 9, len: 2, partial: true });
        assert_eq!(valid("bytes=-3", 10), ByteRange { start: 7, end: 9, len: 3, partial: true });
        assert_eq!(valid("bytes=-99", 10).start, 0);
        assert!(matches!(parse_range("bytes=10-", 10), Some(RangeParse::Unsatisfiable)));
        assert!(matches!(parse_range("bytes=-0", 10), Some(RangeParse::Unsatisfiable)));
        assert!(parse_range("bytes=4-2", 10).is_none());
        assert!(parse_range("bytes=0-1,4-5", 10).is_none());
        assert!(parse_range("items=0-1", 10).is_none());
    }
}
