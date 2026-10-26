//! Проверка прокси/VPN до загрузки.
//!
//! yt-dlp берёт прокси из HTTP(S)_PROXY, а если их нет — из системных настроек Windows.
//! Когда там локальный VPN-клиент (127.0.0.1:10808 и т.п.), а он не запущен, каждое видео
//! падает с «Unable to connect to proxy». Лучше сразу сказать об этом и подождать.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Proxy {
    pub host: String,
    pub port: u16,
    /// откуда взят: HTTPS_PROXY, системные настройки Windows…
    pub source: String,
}

impl Proxy {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn is_local(&self) -> bool {
        let h = self.host.to_ascii_lowercase();
        h == "localhost" || h.starts_with("127.") || h == "::1" || h == "[::1]"
    }

    /// Порт отвечает на TCP-подключение.
    pub fn alive(&self) -> bool {
        let timeout = Duration::from_millis(if self.is_local() { 800 } else { 3000 });
        let host = self.host.trim_start_matches('[').trim_end_matches(']');
        let Ok(addrs) = (host, self.port).to_socket_addrs() else { return false };
        addrs.into_iter().any(|a| TcpStream::connect_timeout(&a, timeout).is_ok())
    }

    pub fn describe(&self) -> String {
        if self.is_local() {
            format!("VPN/прокси {} ({}) не запущен", self.addr(), self.source)
        } else {
            format!("прокси {} ({}) не отвечает", self.addr(), self.source)
        }
    }
}

/// «http://user:pw@127.0.0.1:10808/» → host, port.
fn parse(url: &str, default_port: u16) -> Option<(String, u16)> {
    let mut s = url.trim();
    if let Some(i) = s.find("://") {
        s = &s[i + 3..];
    }
    if let Some(i) = s.rfind('@') {
        s = &s[i + 1..];
    }
    let s = s.split('/').next()?;
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        let port = tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(default_port);
        return Some((format!("[{h}]"), port));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok()?)),
        None => Some((s.to_string(), default_port)),
    }
}

fn env_any(names: &[&str]) -> Option<(String, String)> {
    names.iter().find_map(|n| {
        std::env::var(n).ok().filter(|v| !v.trim().is_empty()).map(|v| (n.to_string(), v))
    })
}

/// Прокси, которым воспользуется yt-dlp для https://www.youtube.com.
pub fn current_proxy() -> Option<Proxy> {
    // Как urllib.request.getproxies(): сначала переменные окружения, затем реестр Windows.
    if let Some((name, val)) =
        env_any(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"])
    {
        let default_port = if val.starts_with("socks") { 1080 } else { 80 };
        let (host, port) = parse(&val, default_port)?;
        return Some(Proxy { host, port, source: name });
    }
    system_proxy()
}

#[cfg(windows)]
fn system_proxy() -> Option<Proxy> {
    use winreg::enums::HKEY_CURRENT_USER;
    let key = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    // «127.0.0.1:10808» или «http=127.0.0.1:8080;https=127.0.0.1:8080;socks=…»
    let pick = if server.contains('=') {
        let parts: Vec<(&str, &str)> = server.split(';').filter_map(|p| p.split_once('=')).collect();
        ["https", "http", "socks"]
            .iter()
            .find_map(|want| parts.iter().find(|(k, _)| k.eq_ignore_ascii_case(want)).map(|(_, v)| *v))?
            .to_string()
    } else {
        server
    };
    let (host, port) = parse(&pick, 80)?;
    Some(Proxy { host, port, source: "системный прокси Windows".into() })
}

#[cfg(not(windows))]
fn system_proxy() -> Option<Proxy> {
    None
}

/// Обрыв связи (переключили VPN, пропал Wi-Fi): такое видео/трек надо повторить, а не считать ошибкой.
pub fn is_network_error(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    [
        "unable to connect to proxy", "connection reset", "connection aborted", "connection refused", "timed out",
        "temporary failure in name resolution", "getaddrinfo failed", "name or service not known", "failed to resolve",
        "network is unreachable", "no route to host", "remote end closed connection", "eof occurred in violation",
        "unexpected_eof", "winerror 10054", "winerror 10053", "winerror 10060", "winerror 10065", "errno 11001",
        "errno 11004", "connectionerror", "incompleteread", "transporterror", "curl: (6)", "curl: (7)", "curl: (28)",
        "curl: (35)", "curl: (52)", "curl: (56)", "сеть:",
    ]
    .iter()
    .any(|k| t.contains(k))
}

/// Есть ли выход в интернет (через тот же прокси/VPN, что и у yt-dlp).
pub fn online() -> bool {
    let proxy = current_proxy();
    ["https://www.youtube.com/generate_204", "https://www.gstatic.com/generate_204"].iter().any(|url| {
        let mut c = crate::appinfo::command("curl.exe");
        c.args(["-s", "-o", "NUL", "-m", "8", "-w", "%{http_code}"]);
        if let Some(p) = &proxy {
            c.args(["-x", &format!("http://{}", p.addr())]);
        }
        c.arg(url).output().is_ok_and(|o| {
            let code = String::from_utf8_lossy(&o.stdout);
            code.starts_with('2') || code.starts_with('3')
        })
    })
}

/// Ошибка yt-dlp из-за недоступного прокси.
pub fn is_proxy_error(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("unable to connect to proxy") || t.contains("proxyerror") || t.contains("proxy error")
        || (t.contains("proxy") && (t.contains("refused") || t.contains("10061")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse("http://127.0.0.1:10808", 80), Some(("127.0.0.1".into(), 10808)));
        assert_eq!(parse("socks5://u:p@localhost:1080/", 80), Some(("localhost".into(), 1080)));
        assert_eq!(parse("proxy.example.com", 80), Some(("proxy.example.com".into(), 80)));
        assert_eq!(parse("http://[::1]:3128", 80), Some(("[::1]".into(), 3128)));
    }

    #[test]
    fn errors() {
        assert!(is_proxy_error("ERROR: Unable to download: ('Unable to connect to proxy', ...)"));
        assert!(!is_proxy_error("HTTP Error 403: Forbidden"));
    }
}
