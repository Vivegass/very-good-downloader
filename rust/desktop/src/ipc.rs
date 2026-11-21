//! Связь с браузером и вторым запуском программы.
//!
//! * Окно слушает `127.0.0.1:38517`. Расширение браузера шлёт туда `POST /add` со ссылкой —
//!   загрузка начинается сразу, без лишних окон.
//! * Если окно не запущено, расширение открывает ссылку `vgd://add?url=…`: Windows запускает
//!   программу (протокол `vgd` регистрируется в HKCU при каждом запуске окна и установщиком).
//! * Второй запуск программы не открывает второе окно, а передаёт ссылки первому.
//!   Первый узнаётся по именованному мьютексу Windows (`single_instance`), который ставится
//!   в самом начале `main`, а порт занимается сразу за ним — ещё до создания окна egui.
//!   Ссылки, пришедшие до появления окна, ждут в канале (`Incoming`).
//! * `POST /add` отвечает номером заявки (`ticket`); по `GET /status?t=N` расширение и Spotify
//!   узнают, что с ней: `queued` → `running` → `done` / `error`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use crate::appinfo::APP_VERSION;

pub const PORT: u16 = 38517;

/// Версия расширения, которое последним дало о себе знать (`POST /hello`).
static EXTENSION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn extension_version() -> Option<String> {
    EXTENSION.lock().ok().and_then(|v| v.clone())
}

/// Что пришло извне: ссылки (с номером заявки, если пришли по сети) или «покажи окно».
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Add { urls: Vec<String>, ticket: Option<u64> },
    Show,
}

/// Заявки `POST /add`: номер → (состояние, пояснение). Последние 300.
static TICKETS: std::sync::Mutex<Vec<(u64, &'static str, String)>> = std::sync::Mutex::new(Vec::new());
static NEXT_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn new_ticket() -> u64 {
    let t = NEXT_TICKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    set_ticket(t, "queued", "");
    t
}

/// Обновить состояние заявки: `queued`, `running`, `done`, `error`.
pub fn set_ticket(t: u64, state: &'static str, text: &str) {
    let Ok(mut v) = TICKETS.lock() else { return };
    let text: String = text.chars().take(200).collect();
    match v.iter_mut().find(|e| e.0 == t) {
        Some(e) => {
            e.1 = state;
            e.2 = text;
        }
        None => {
            v.push((t, state, text));
            if v.len() > 300 {
                v.remove(0);
            }
        }
    }
}

fn ticket(t: u64) -> Option<(&'static str, String)> {
    TICKETS.lock().ok()?.iter().find(|e| e.0 == t).map(|e| (e.1, e.2.clone()))
}

/// Именованный мьютекс Windows: `true` — мы первая копия программы (мьютекс наш до выхода),
/// `false` — уже запущена другая. Повторный вызов из второй копии пробует снова
/// (первая могла как раз закрыться).
#[cfg(windows)]
pub fn single_instance() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *const std::ffi::c_void, owner: i32, name: *const u16) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
    }
    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = r"Local\VeryGoodDownloader.SingleInstance".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() {
            return true; // не смогли проверить — работаем как раньше
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(h);
            return false;
        }
        // Ручку не закрываем: мьютекс живёт, пока жива программа.
        true
    }
}

#[cfg(not(windows))]
pub fn single_instance() -> bool {
    true
}

fn addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], PORT))
}

/// Ссылки из командной строки: обычные http(s) и `vgd://add?url=…`.
pub fn launch_urls<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .filter_map(|a| {
            let a = a.trim().to_string();
            if a.to_ascii_lowercase().starts_with("vgd:") {
                decode_vgd(&a)
            } else if is_web_url(&a) {
                Some(a)
            } else {
                None
            }
        })
        .collect()
}

pub fn is_web_url(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    (l.starts_with("http://") || l.starts_with("https://")) && s.len() > 10 && !s.contains(char::is_whitespace)
}

/// `vgd://add?url=https%3A%2F%2F…` (браузер иногда добавляет `/` перед `?`) → ссылка.
pub fn decode_vgd(s: &str) -> Option<String> {
    let i = s.find("url=")?;
    let raw = &s[i + 4..];
    let raw = raw.split('&').next().unwrap_or(raw);
    let url = percent_decode(raw);
    let url = url.trim_end_matches('/').to_string();
    is_web_url(&url).then_some(url)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Передать ссылки уже запущенной программе; без ссылок — попросить показать окно (`show`)
/// или ничего (запуск в фоне). `true` — программа есть и всё приняла (тогда второму выходить).
pub fn forward(urls: &[String], show: bool) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&addr(), Duration::from_millis(400)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let path = if urls.is_empty() && show { "/show" } else { "/add" };
    let body = serde_json::json!({ "urls": urls }).to_string();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-VGD: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut resp = String::new();
    let _ = s.read_to_string(&mut resp);
    resp.starts_with("HTTP/1.1 200")
}

/// Запустить приём ссылок. `false` — порт занят (например, окно уже открыто).
pub fn serve(on_urls: impl Fn(Incoming) + Send + Sync + 'static) -> bool {
    let Ok(listener) = TcpListener::bind(addr()) else { return false };
    let on_urls = std::sync::Arc::new(on_urls);
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let cb = on_urls.clone();
            std::thread::spawn(move || {
                let _ = handle(conn, &*cb);
            });
        }
    });
    true
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn read_request(conn: &mut TcpStream) -> Option<Request> {
    conn.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = conn.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0)
        .min(256 * 1024);
    let mut body = buf[head_end..].to_vec();
    while body.len() < len {
        let n = conn.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    Some(Request { method, path, headers, body })
}

/// Ответ; для приложения Spotify (`cors`) — с заголовками CORS, иначе его страница ответ не прочитает.
fn respond_to(conn: &mut TcpStream, cors: Option<&str>, status: &str, body: &str) {
    let cors = cors
        .map(|o| format!("Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Methods: GET, POST\r\nAccess-Control-Allow-Headers: X-VGD, Content-Type\r\nAccess-Control-Max-Age: 600\r\nVary: Origin\r\n"))
        .unwrap_or_default();
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{cors}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = conn.write_all(resp.as_bytes());
}

/// Страница приложения Spotify (расширение Spicetify `vgd.js`).
const SPOTIFY_ORIGIN: &str = "https://xpui.app.spotify.com";

/// Запросы только от расширения браузера или второго запуска программы:
/// обычная веб-страница не может добавить заголовок `X-VGD` без разрешения (CORS),
/// а `Origin` у неё — адрес сайта, а не `chrome-extension://`.
fn allowed(req: &Request) -> bool {
    if req.header("x-vgd") != Some("1") {
        return false;
    }
    match req.header("origin") {
        None => true,
        Some(o) => {
            o.starts_with("chrome-extension://") || o.starts_with("moz-extension://") || o.starts_with("extension://") || o == SPOTIFY_ORIGIN
        }
    }
}

fn handle(mut conn: TcpStream, on_urls: &dyn Fn(Incoming)) -> Option<()> {
    let req = read_request(&mut conn)?;
    let path = req.path.split('?').next().unwrap_or("");
    let cors = (req.header("origin") == Some(SPOTIFY_ORIGIN)).then_some(SPOTIFY_ORIGIN);
    match (req.method.as_str(), path) {
        // Предварительный запрос CORS из приложения Spotify (заголовок X-VGD — «непростой»).
        ("OPTIONS", _) if cors.is_some() => {
            respond_to(&mut conn, cors, "204 No Content", "");
        }
        ("GET", "/ping") if allowed(&req) => {
            respond_to(&mut conn, cors, "200 OK", &format!("{{\"app\":\"vgd\",\"version\":\"{APP_VERSION}\"}}"));
        }
        ("GET", "/status") if allowed(&req) => {
            let t: u64 = req.path.split_once("t=").and_then(|(_, v)| v.split('&').next()?.parse().ok()).unwrap_or(0);
            let body = match ticket(t) {
                Some((state, text)) => serde_json::json!({ "state": state, "text": text }).to_string(),
                None => "{\"state\":\"unknown\",\"text\":\"\"}".to_string(),
            };
            respond_to(&mut conn, cors, "200 OK", &body);
        }
        ("POST", "/show") if allowed(&req) => {
            on_urls(Incoming::Show);
            respond_to(&mut conn, cors, "200 OK", "{\"ok\":true}");
        }
        ("POST", "/hello") if allowed(&req) => {
            let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            if let (Ok(mut e), Some(ver)) = (EXTENSION.lock(), v.get("version").and_then(|x| x.as_str())) {
                *e = Some(ver.chars().take(20).collect());
            }
            respond_to(&mut conn, cors, "200 OK", "{\"ok\":true}");
        }
        // Вход в YouTube / SoundCloud: расширение присылает куки вошедшего браузера (только если в программе
        // включено «брать звук от аккаунта»). Только от расширения — не от второго запуска и не от Spotify.
        // Сначала расширение спрашивает, нужен ли вход (GET /cookies?service=…): если нет — куки не присылает.
        ("GET", "/cookies") if allowed(&req) && req.header("origin").is_some_and(|o| o != SPOTIFY_ORIGIN) => {
            let service = req.path.split_once("service=").map(|(_, v)| v.split('&').next().unwrap_or("")).unwrap_or("");
            let wanted = crate::accounts::wanted(&crate::settings::Settings::load(), service);
            respond_to(&mut conn, cors, "200 OK", if wanted { "{\"ok\":true,\"state\":\"wanted\"}" } else { "{\"ok\":true,\"state\":\"unwanted\"}" });
        }
        ("POST", "/cookies") if allowed(&req) && req.header("origin").is_some_and(|o| o != SPOTIFY_ORIGIN) => {
            let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let service = v.get("service").and_then(|s| s.as_str()).unwrap_or("");
            let s = crate::settings::Settings::load();
            let body = if !crate::accounts::wanted(&s, service) {
                serde_json::json!({ "ok": true, "state": "unwanted" })
            } else {
                match crate::accounts::save_json(service, v.get("cookies").unwrap_or(&serde_json::Value::Null)) {
                    Ok((n, logged)) => serde_json::json!({ "ok": true, "state": "saved", "count": n, "logged": logged }),
                    Err(e) => serde_json::json!({ "ok": false, "state": "error", "error": e }),
                }
            };
            respond_to(&mut conn, cors, "200 OK", &body.to_string());
        }
        ("POST", "/add") if allowed(&req) => {
            let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let mut urls: Vec<String> = Vec::new();
            if let Some(u) = v.get("url").and_then(|u| u.as_str()) {
                urls.push(u.to_string());
            }
            if let Some(a) = v.get("urls").and_then(|a| a.as_array()) {
                urls.extend(a.iter().filter_map(|u| u.as_str()).map(String::from));
            }
            let mut urls: Vec<String> = urls.into_iter().map(|u| u.trim().to_string()).filter(|u| is_web_url(u)).collect();
            // Список треков со страницы (VK): сохраняем в файл и ставим в очередь ссылкой vgdlist:.
            if let Some(tracks) = v.get("tracks").filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
                let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("VK");
                let service = v.get("service").and_then(|n| n.as_str()).unwrap_or("VK");
                if let Some(u) = crate::music::save_list(name, service, tracks, v.get("diag")) {
                    urls.push(u);
                }
            }
            let n = urls.len();
            if n == 0 {
                respond_to(&mut conn, cors, "200 OK", "{\"ok\":true,\"added\":0}");
            } else {
                let t = new_ticket();
                on_urls(Incoming::Add { urls, ticket: Some(t) });
                respond_to(&mut conn, cors, "200 OK", &format!("{{\"ok\":true,\"added\":{n},\"ticket\":{t}}}"));
            }
        }
        _ => respond_to(&mut conn, cors, "403 Forbidden", "{\"ok\":false}"),
    }
    Some(())
}

fn protocol_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// Протокол `vgd://` уже ведёт на эту программу.
#[cfg(windows)]
pub fn protocol_registered(exe: &Path) -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Classes\vgd\shell\open\command")
        .and_then(|k| k.get_value::<String, _>(""))
        .is_ok_and(|v| v.eq_ignore_ascii_case(&protocol_command(exe)))
}

#[cfg(not(windows))]
pub fn protocol_registered(_exe: &Path) -> bool {
    false
}

/// Протокол `vgd://` в HKCU: по нему браузер запускает программу, если окно не открыто.
/// Ставится установщиком или кнопкой в настройках — только по действию пользователя.
#[cfg(windows)]
pub fn register_protocol(exe: &Path) {
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let command = protocol_command(exe);
    if let Ok((k, _)) = hkcu.create_subkey(r"Software\Classes\vgd") {
        let _ = k.set_value("", &"URL:Very Good Downloader");
        let _ = k.set_value("URL Protocol", &"");
    }
    if let Ok((k, _)) = hkcu.create_subkey(r"Software\Classes\vgd\DefaultIcon") {
        let _ = k.set_value("", &format!("\"{}\",0", exe.display()));
    }
    if let Ok((k, _)) = hkcu.create_subkey(r"Software\Classes\vgd\shell\open\command") {
        let _ = k.set_value("", &command);
    }
}

#[cfg(not(windows))]
pub fn register_protocol(_exe: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vgd_links() {
        assert_eq!(
            decode_vgd("vgd://add?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DjNQXAC9IVRw").as_deref(),
            Some("https://www.youtube.com/watch?v=jNQXAC9IVRw")
        );
        assert_eq!(
            decode_vgd("vgd://add/?url=https%3A%2F%2Fwww.pinterest.com%2Fpin%2F123%2F").as_deref(),
            Some("https://www.pinterest.com/pin/123")
        );
        assert_eq!(decode_vgd("vgd://add?url=javascript%3Aalert(1)"), None);
        let args = vec!["--page=log".to_string(), "https://vimeo.com/1".to_string(), "vgd://add?url=https%3A%2F%2Fx.com%2Fa".to_string()];
        assert_eq!(launch_urls(args), vec!["https://vimeo.com/1".to_string(), "https://x.com/a".to_string()]);
    }

    /// Запрос из приложения Spotify: предварительный OPTIONS и POST /add — с заголовками CORS;
    /// чужой сайт — отказ.
    #[test]
    fn spotify_cors() {
        let ask = |req: String| -> String {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let a = l.local_addr().unwrap();
            let t = std::thread::spawn(move || {
                let (c, _) = l.accept().unwrap();
                let _ = handle(c, &|_| {});
            });
            let mut s = TcpStream::connect(a).unwrap();
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            t.join().unwrap();
            out
        };
        let pre = ask(format!("OPTIONS /add HTTP/1.1\r\nOrigin: {SPOTIFY_ORIGIN}\r\nAccess-Control-Request-Headers: x-vgd,content-type\r\n\r\n"));
        assert!(pre.starts_with("HTTP/1.1 204") && pre.contains("Access-Control-Allow-Headers: X-VGD"), "{pre}");
        let body = r#"{"name":"P","service":"Spotify","tracks":[]}"#;
        let post = ask(format!("POST /add HTTP/1.1\r\nOrigin: {SPOTIFY_ORIGIN}\r\nX-VGD: 1\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        assert!(post.starts_with("HTTP/1.1 200") && post.contains(&format!("Access-Control-Allow-Origin: {SPOTIFY_ORIGIN}")), "{post}");
        let evil = ask("OPTIONS /add HTTP/1.1\r\nOrigin: https://evil.example\r\n\r\n".into());
        assert!(evil.starts_with("HTTP/1.1 403") && !evil.contains("Allow-Origin"), "{evil}");
    }

    /// `POST /add` даёт номер заявки, `GET /status` отдаёт её состояние; `/show` — просьба показать окно.
    #[test]
    fn tickets_and_show() {
        let ask = |req: String| -> (String, Vec<Incoming>) {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let a = l.local_addr().unwrap();
            let t = std::thread::spawn(move || {
                let (c, _) = l.accept().unwrap();
                let got = std::sync::Mutex::new(Vec::new());
                let _ = handle(c, &|m| got.lock().unwrap().push(m));
                got.into_inner().unwrap()
            });
            let mut s = TcpStream::connect(a).unwrap();
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            (out, t.join().unwrap())
        };
        let body = r#"{"url":"https://vimeo.com/1"}"#;
        let (out, got) = ask(format!("POST /add HTTP/1.1\r\nX-VGD: 1\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        let v: serde_json::Value = serde_json::from_str(out.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        let t = v["ticket"].as_u64().expect("номер заявки");
        assert_eq!(got, vec![Incoming::Add { urls: vec!["https://vimeo.com/1".into()], ticket: Some(t) }]);
        let (st, _) = ask(format!("GET /status?t={t} HTTP/1.1\r\nX-VGD: 1\r\n\r\n"));
        assert!(st.contains(r#""state":"queued""#), "{st}");
        set_ticket(t, "done", "файлов: 1");
        let (st, _) = ask(format!("GET /status?t={t} HTTP/1.1\r\nX-VGD: 1\r\n\r\n"));
        assert!(st.contains(r#""state":"done""#), "{st}");
        let (_, got) = ask("POST /show HTTP/1.1\r\nX-VGD: 1\r\nContent-Length: 0\r\n\r\n".into());
        assert_eq!(got, vec![Incoming::Show]);
        // Без заголовка X-VGD (обычная страница) — отказ.
        let (out, got) = ask("POST /show HTTP/1.1\r\nContent-Length: 0\r\n\r\n".into());
        assert!(out.starts_with("HTTP/1.1 403") && got.is_empty());
    }
}
