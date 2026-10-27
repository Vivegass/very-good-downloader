//! Поиск музыки в VK от аккаунта пользователя — «невидимый браузер».
//!
//! Отдельный процесс программы (`--vk-agent`) открывает vk.com в скрытом окне WebView2 (движок Edge,
//! есть в Windows 10/11) с той же папкой данных, что и окно входа (`--vk-login`). В странице работает
//! `vk_agent.js`: `audio.search` через API, которым пользуется сам сайт, и расшифровка ссылок на звук.
//! Связь с процессом — строки JSON: запросы в stdin, ответы в stdout. Пароль программа не видит:
//! пользователь входит на самой странице vk.com, сессия хранится в папке WebView2.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::appinfo::{command, data_dir};

const AGENT_JS: &str = include_str!("vk_agent.js");

/// Найденный в VK трек: ссылка на звук (HLS), название, исполнитель, длительность, обложка.
#[derive(Debug, Clone)]
pub struct VkHit {
    pub url: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<f64>,
    pub cover: Option<String>,
}

fn webview_dir() -> PathBuf {
    data_dir().join("webview2")
}

/// Файл с последним известным состоянием входа (пишут окно входа и фоновый процесс).
fn state_file() -> PathBuf {
    data_dir().join("vk_state.json")
}

/// Вошёл ли пользователь в VK (по последней проверке).
pub fn logged_in() -> bool {
    std::fs::read_to_string(state_file())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["logged"].as_bool())
        .unwrap_or(false)
}

/// Exe окна программы (у него есть режимы `--vk-login` / `--vk-agent`).
fn gui_exe() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let name = me.file_name()?.to_string_lossy().to_lowercase();
    if name.contains("vgd-gui") || name.contains("very good downloader") {
        return Some(me);
    }
    ["Very Good Downloader.exe", "vgd-gui.exe"].iter().map(|n| me.with_file_name(n)).find(|p| p.exists())
}

/// Открыть окно входа в VK (пользователь входит сам, на странице vk.com).
pub fn open_login() -> Result<(), String> {
    let exe = gui_exe().ok_or("не найдено окно программы")?;
    std::process::Command::new(exe).arg("--vk-login").spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------- клиент: процесс-агент и запросы к нему ----------

struct Agent {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next: u64,
}

static AGENT: Mutex<Option<Agent>> = Mutex::new(None);
/// Последняя неудача запуска (нет входа, нет связи): не пробуем снова для каждого трека.
static FAILED: Mutex<Option<(Instant, String)>> = Mutex::new(None);

fn start_agent() -> Result<Agent, String> {
    let exe = gui_exe().ok_or("не найдено окно программы для поиска в VK")?;
    let mut child = command(exe)
        .arg("--vk-agent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("не запустился поиск VK: {e}"))?;
    let stdin = child.stdin.take().ok_or("нет stdin")?;
    let stdout = child.stdout.take().ok_or("нет stdout")?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let mut a = Agent { child, stdin, rx, next: 1 };
    // Ждём, пока страница VK загрузится и скажет, есть ли вход.
    let end = Instant::now() + Duration::from_secs(40);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match a.rx.recv_timeout(left) {
            Ok(v) if v["type"] == "state" => {
                if v["logged"].as_bool() == Some(true) && v["api"].as_bool() == Some(true) {
                    return Ok(a);
                }
                if v["logged"].as_bool() == Some(false) {
                    let _ = a.child.kill();
                    return Err("нет входа в VK — «настройки → музыка → войти в VK»".into());
                }
            }
            Ok(_) => {}
            Err(_) => {
                let _ = a.child.kill();
                return Err("страница VK не загрузилась (нет связи или нужен WebView2)".into());
            }
        }
    }
}

/// Поиск треков в VK. Первый вызов запускает фоновый процесс (несколько секунд), дальше — быстро.
pub fn search(q: &str, count: u32) -> Result<Vec<VkHit>, String> {
    let mut guard = AGENT.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_mut().is_some_and(|a| a.child.try_wait().ok().flatten().is_some()) {
        *guard = None; // процесс завершился — запустим заново
    }
    if guard.is_none() {
        let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, e)) = failed.as_ref() {
            if at.elapsed() < Duration::from_secs(300) {
                return Err(e.clone());
            }
        }
        match start_agent() {
            Ok(a) => {
                *failed = None;
                *guard = Some(a);
            }
            Err(e) => {
                *failed = Some((Instant::now(), e.clone()));
                return Err(e);
            }
        }
    }
    let a = guard.as_mut().unwrap();
    let rid = a.next;
    a.next += 1;
    let req = json!({ "rid": rid, "q": q, "count": count });
    if writeln!(a.stdin, "{req}").and_then(|_| a.stdin.flush()).is_err() {
        *guard = None;
        return Err("поиск VK прервался".into());
    }
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        match a.rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(v) if v["rid"].as_u64() == Some(rid) => {
                if let Some(e) = v["error"].as_str() {
                    return Err(format!("VK: {e}"));
                }
                return Ok(v["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| {
                        Some(VkHit {
                            url: t["url"].as_str()?.to_string(),
                            title: t["title"].as_str().unwrap_or("").to_string(),
                            artist: t["artist"].as_str().unwrap_or("").to_string(),
                            duration: t["duration"].as_f64(),
                            cover: t["cover"].as_str().map(String::from),
                        })
                    })
                    .collect());
            }
            Ok(_) => {}
            Err(_) => {
                let _ = a.child.kill();
                *guard = None;
                return Err("VK не ответил за 30 с".into());
            }
        }
    }
}

// ---------- сам процесс с окном WebView2 ----------

enum UserEvent {
    Eval(String),
    Quit,
}

/// Режим процесса: `login` — видимое окно входа; иначе скрытый агент поиска (stdin/stdout).
pub fn run_window(login: bool) -> Result<(), String> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::{WebContext, WebViewBuilder};

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Вход в VK — Very Good Downloader")
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 760.0))
        .with_visible(login)
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    let mut ctx = WebContext::new(Some(webview_dir()));
    let ipc = move |req: wry::http::Request<String>| {
        let body = req.body();
        if let Ok(v) = serde_json::from_str::<Value>(body) {
            if v["type"] == "state" {
                let _ = std::fs::write(state_file(), json!({ "logged": v["logged"], "checked": crate_time() }).to_string());
            }
        }
        if !login {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{body}");
            let _ = out.flush();
        }
    };
    let url = if login { "https://vk.com/" } else { "https://vk.com/audio" };
    let webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url(url)
        .with_initialization_script(AGENT_JS)
        .with_ipc_handler(ipc)
        .build(&window)
        .map_err(|e| e.to_string())?;

    if !login {
        // Запросы программы: строки JSON {rid, q, count}. Закрыли stdin (программа вышла) — выходим.
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                let js = format!(
                    "window.__vgdSearch && window.__vgdSearch({}, {}, {})",
                    v["rid"],
                    serde_json::to_string(v["q"].as_str().unwrap_or("")).unwrap_or_default(),
                    v["count"].as_u64().unwrap_or(10)
                );
                if proxy.send_event(UserEvent::Eval(js)).is_err() {
                    break;
                }
            }
            let _ = proxy.send_event(UserEvent::Quit);
        });
    }

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::Eval(js)) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::Quit) | Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn crate_time() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
