//! HTTP-запросы программы (списки треков, API Яндекса/Spotify/Deezer, обложки, проверка связи).
//!
//! Раньше всё шло через `curl.exe` из Windows; на Android curl нет, поэтому — свой клиент на `ureq` + rustls
//! (корневые сертификаты встроены, системные не нужны). Поведение как у прежнего curl:
//! - редиректы проходятся (`-L`), сжатие gzip (`--compressed`), общий лимит времени (`--max-time`);
//! - ответ с кодом 4xx/5xx — не ошибка, тело отдаётся как есть (curl без `-f`): API пишут ошибку в JSON;
//! - повтор при временных сбоях (`--retry N --retry-delay 2`): обрыв/таймаут, коды 408, 429, 500, 502–504;
//! - прокси — из `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`, как у curl; можно задать явно (`proxy`).
//!
//! Ошибки сети начинаются с «сеть:» — по этому `net::is_network_error` понимает, что запрос надо повторить позже.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use ureq::ResponseExt;

/// Самый большой ответ, который читаем в память (страницы Apple Music — несколько МБ).
const MAX_TEXT: u64 = 64 << 20;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Method {
    Get,
    Post,
}

/// Запрос: `http::get(url).ua(UA).timeout(40).retries(3).text()`.
#[derive(Clone, Debug)]
pub struct Request {
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    form: Option<Vec<(String, String)>>,
    timeout: u64,
    connect_timeout: Option<u64>,
    retries: u32,
    follow: bool,
    proxy: Option<String>,
}

pub fn get(url: &str) -> Request {
    Request::new(Method::Get, url)
}

pub fn post(url: &str) -> Request {
    Request::new(Method::Post, url)
}

/// Ответ: код и тело.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    /// адрес после всех редиректов
    pub url: String,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn is_transient_status(code: u16) -> bool {
    matches!(code, 408 | 429 | 500 | 502 | 503 | 504)
}

impl Request {
    fn new(method: Method, url: &str) -> Self {
        Request { method, url: url.to_string(), headers: Vec::new(), form: None, timeout: 40, connect_timeout: None, retries: 0, follow: true, proxy: None }
    }

    /// Заголовок `Имя: значение`.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Заголовок строкой «Имя: значение» (как `curl -H`).
    pub fn header_line(self, line: &str) -> Self {
        match line.split_once(':') {
            Some((k, v)) => self.header(k.trim(), v.trim()),
            None => self,
        }
    }

    pub fn ua(self, ua: &str) -> Self {
        self.header("User-Agent", ua)
    }

    /// `curl -u login:пароль`.
    pub fn basic_auth(self, user: &str, password: &str) -> Self {
        use base64::Engine;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
        self.header("Authorization", &format!("Basic {token}"))
    }

    /// Тело `application/x-www-form-urlencoded` (`curl --data-urlencode k=v`).
    pub fn form(mut self, pairs: &[(&str, &str)]) -> Self {
        self.form = Some(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        self
    }

    /// Лимит на весь запрос, секунды (`--max-time`).
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout = secs.max(1);
        self
    }

    /// Лимит только на установку соединения, секунды (`--connect-timeout`): мёртвый хост
    /// отпускает запрос быстрее, чем полный таймаут, а глобальный лимит не трогает.
    pub fn connect_timeout(mut self, secs: u64) -> Self {
        self.connect_timeout = Some(secs.max(1));
        self
    }

    /// Сколько раз повторить при временном сбое (`--retry`).
    pub fn retries(mut self, n: u32) -> Self {
        self.retries = n;
        self
    }

    /// Не ходить по редиректам (curl без `-L`).
    pub fn no_follow(mut self) -> Self {
        self.follow = false;
        self
    }

    /// Явный прокси (`http://host:port`, `socks5://…`) вместо переменных окружения.
    pub fn proxy(mut self, proxy: &str) -> Self {
        self.proxy = Some(proxy.to_string());
        self
    }

    fn agent(&self) -> Result<ureq::Agent, String> {
        let proxy = match &self.proxy {
            Some(p) => Some(ureq::Proxy::new(p).map_err(|e| format!("сеть: прокси {p}: {e}"))?),
            None => ureq::Proxy::try_from_env(),
        };
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(self.timeout)))
            .timeout_connect(self.connect_timeout.map(Duration::from_secs))
            .http_status_as_error(false)
            .max_redirects(if self.follow { 10 } else { 0 })
            .save_redirect_history(false)
            .proxy(proxy)
            .build();
        Ok(config.into())
    }

    /// Одна попытка; `sink` получает тело по частям. Возвращает код и итоговый адрес.
    fn once(&self, sink: &mut dyn FnMut(&mut dyn Read) -> std::io::Result<()>) -> Result<(u16, String), String> {
        let agent = self.agent()?;
        let result = match self.method {
            Method::Get => {
                let mut r = agent.get(&self.url);
                for (k, v) in &self.headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            Method::Post => {
                let mut r = agent.post(&self.url);
                for (k, v) in &self.headers {
                    r = r.header(k, v);
                }
                match &self.form {
                    Some(f) => r.send_form(f.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
                    None => r.send_empty(),
                }
            }
        };
        let mut resp = result.map_err(|e| format!("сеть: {}", describe(&e)))?;
        let status = resp.status().as_u16();
        let url = resp.get_uri().to_string();
        let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
        sink(&mut reader).map_err(|e| format!("сеть: {e}"))?;
        Ok((status, url))
    }

    /// С повторами при временных сбоях.
    fn run(&self, sink: &mut dyn FnMut(&mut dyn Read) -> std::io::Result<()>) -> Result<(u16, String), String> {
        let mut attempt = 0;
        loop {
            let r = self.once(sink);
            let transient = match &r {
                Ok((code, _)) => is_transient_status(*code),
                Err(_) => true,
            };
            if !transient || attempt >= self.retries {
                return r;
            }
            attempt += 1;
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Ответ целиком (до 64 МБ).
    pub fn send(&self) -> Result<Response, String> {
        let mut body = Vec::new();
        let (status, url) = self.run(&mut |r| {
            body.clear();
            r.take(MAX_TEXT).read_to_end(&mut body).map(|_| ())
        })?;
        Ok(Response { status, body, url })
    }

    /// Тело ответа текстом (как `curl -s`: код ответа не проверяется).
    pub fn text(&self) -> Result<String, String> {
        self.send().map(|r| r.text())
    }

    /// Тело ответа как JSON.
    pub fn json(&self) -> Result<serde_json::Value, String> {
        let r = self.send()?;
        serde_json::from_slice(&r.body).map_err(|_| format!("непонятный ответ от {}", host(&self.url)))
    }

    /// Только код ответа (тело не читается целиком).
    pub fn status(&self) -> Result<u16, String> {
        self.run(&mut |r| std::io::copy(&mut r.take(1 << 16), &mut std::io::sink()).map(|_| ())).map(|(s, _)| s)
    }

    /// Адрес после всех редиректов (`curl -w %{url_effective}`): короткие ссылки spotify.link и т. п.
    pub fn final_url(&self) -> Result<String, String> {
        self.run(&mut |r| std::io::copy(&mut r.take(1 << 20), &mut std::io::sink()).map(|_| ())).map(|(_, u)| u)
    }

    /// Скачать в файл (`curl -o`). Файл пишется рядом с расширением `.part` и переименовывается в конце.
    pub fn download(&self, path: &Path) -> Result<u16, String> {
        let part = path.with_extension(format!("{}.part", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
        let result = self.run(&mut |r| {
            let mut f = std::fs::File::create(&part)?;
            std::io::copy(r, &mut f)?;
            Ok(())
        });
        let status = match result {
            Ok((s, _)) => s,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        };
        std::fs::rename(&part, path).map_err(|e| e.to_string())?;
        Ok(status)
    }
}

/// Хост из ссылки — для сообщений об ошибках.
pub fn host(url: &str) -> &str {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url)
}

/// Текст ошибки с ключевыми словами, по которым `net::is_network_error` узнаёт обрыв связи.
fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::HostNotFound => "failed to resolve host".into(),
        ureq::Error::Io(io) => match io.kind() {
            std::io::ErrorKind::ConnectionRefused => format!("connection refused ({io})"),
            std::io::ErrorKind::ConnectionReset => format!("connection reset ({io})"),
            std::io::ErrorKind::ConnectionAborted => format!("connection aborted ({io})"),
            std::io::ErrorKind::TimedOut => format!("timed out ({io})"),
            _ => io.to_string(),
        },
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Маленький HTTP-сервер на один-несколько запросов: отвечает по очереди готовыми ответами.
    fn serve(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for resp in responses {
                let (mut s, _) = l.accept().unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut head = String::new();
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                head.push_str(&String::from_utf8_lossy(&body));
                seen.push(head);
                s.write_all(resp.as_bytes()).unwrap();
            }
            seen
        });
        (addr, h)
    }

    fn ok(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    #[test]
    fn get_text_headers_and_ua() {
        let (addr, h) = serve(vec![ok("привет")]);
        let t = get(&format!("{addr}/x")).ua("VGD-test").header_line("X-A: 1").text().unwrap();
        assert_eq!(t, "привет");
        let seen = h.join().unwrap();
        let req = seen[0].to_ascii_lowercase();
        assert!(req.starts_with("get /x "));
        assert!(req.contains("user-agent: vgd-test"));
        assert!(req.contains("x-a: 1"));
    }

    #[test]
    fn error_status_is_not_error_and_body_returned() {
        let (addr, h) = serve(vec!["HTTP/1.1 404 Not Found\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"error\":\"no\"}\n".into()]);
        let r = get(&addr).send().unwrap();
        assert_eq!(r.status, 404);
        assert!(r.text().contains("error"));
        h.join().unwrap();
    }

    #[test]
    fn retries_on_503_then_ok() {
        let busy = "HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
        let (addr, h) = serve(vec![busy, ok("{\"a\":1}")]);
        let v = get(&addr).retries(2).json().unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(h.join().unwrap().len(), 2);
    }

    #[test]
    fn post_form_and_basic_auth() {
        let (addr, h) = serve(vec![ok("{}")]);
        post(&addr).basic_auth("id", "secret").form(&[("grant_type", "client_credentials"), ("q", "а б")]).text().unwrap();
        let req = h.join().unwrap().remove(0);
        assert!(req.starts_with("POST "));
        assert!(req.contains("Basic aWQ6c2VjcmV0"), "{req}");
        assert!(req.contains("grant_type=client_credentials&q=%D0%B0+%D0%B1") || req.contains("q=%D0%B0%20%D0%B1"), "{req}");
    }

    #[test]
    fn follows_redirect_and_reports_final_url() {
        let (addr, h) = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: /final?x=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            ok("done"),
        ]);
        let u = get(&format!("{addr}/short")).final_url().unwrap();
        assert!(u.ends_with("/final?x=1"), "{u}");
        h.join().unwrap();
    }

    #[test]
    fn download_writes_file() {
        let (addr, h) = serve(vec![ok("0123456789")]);
        let p = std::env::temp_dir().join(format!("vgd-http-{}.bin", std::process::id()));
        get(&addr).download(&p).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"0123456789");
        let _ = std::fs::remove_file(&p);
        h.join().unwrap();
    }

    #[test]
    fn refused_is_network_error() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        drop(l);
        let e = get(&addr).timeout(5).text().unwrap_err();
        assert!(crate::net::is_network_error(&e), "{e}");
    }
}

#[cfg(test)]
mod live {
    /// Настоящий HTTPS (TLS, gzip, редирект): `cargo test -p vgd-core --lib -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_https() {
        let v = super::get("https://api.deezer.com/search?q=daft%20punk&limit=1").timeout(20).json().unwrap();
        assert!(v["data"].is_array(), "{v}");
        assert!(crate::net::online());
    }
}
