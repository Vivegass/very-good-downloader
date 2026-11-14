//! Обновление программы через GitHub Releases (`Vivegass/very-good-downloader`) — общее для ПК и Android.
//!
//! Схема релизов (подробно — в rust/README.md):
//! - ПК: тег `v1.11.0`, ассеты `VeryGoodDownloader-Setup-1.11.0.exe` и `VeryGoodDownloader-1.11.0-portable.zip`;
//! - Android: отдельный релиз с тегом `android-v0.3.0`, ассет `VeryGoodDownloader-android-0.3.0-arm64.apk`;
//! - у каждого ассета — SHA-256: поле `digest` в ответе GitHub API (`"sha256:…"`, GitHub считает его сам)
//!   или, если его нет, ассет `<имя>.sha256` рядом. Без контрольной суммы обновление не ставится.
//!
//! Что здесь есть:
//! - [`check`] — есть ли версия новее (черновики пропускаются, предварительные — только с «бета-версиями»);
//! - [`download`] — скачать ассет с докачкой после обрыва, проверить размер и SHA-256;
//! - [`Prefs`] — «проверять автоматически», «бета-версии», время последней проверки, «пропустить версию»
//!   (файл `update.json` в папке данных — отдельно от settings.json).
//!
//! Окна, установки и перезапуска здесь нет: это делает программа для Windows (`vgd::updater`)
//! и приложение для Android (через UniFFI).
//!
//! Адрес API можно подменить переменной окружения `VGD_UPDATE_URL` — только для отладки
//! (локальный mock-сервер со списком релизов); тогда разрешены и ссылки `http://`.

use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::appinfo::data_dir;

/// Репозиторий с релизами.
pub const REPO: &str = "Vivegass/very-good-downloader";
/// Переменная окружения: свой адрес списка релизов (только для отладки).
pub const ENV_URL: &str = "VGD_UPDATE_URL";
/// Автоматически — не чаще раза в 6 часов.
pub const CHECK_EVERY: u64 = 6 * 3600;

/// Адрес списка релизов: GitHub API или `VGD_UPDATE_URL`.
pub fn api_url() -> String {
    match std::env::var(ENV_URL) {
        Ok(u) if !u.trim().is_empty() => u.trim().to_string(),
        _ => format!("https://api.github.com/repos/{REPO}/releases?per_page=30"),
    }
}

/// Страница всех релизов (для ссылки «все версии»).
pub fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases")
}

fn debug_url() -> bool {
    std::env::var(ENV_URL).is_ok_and(|u| !u.trim().is_empty())
}

// ---------- платформа ----------

/// Для какой сборки ищем обновление.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    /// Windows, установленная программа: `VeryGoodDownloader-Setup-<ver>.exe`
    WindowsSetup,
    /// Windows без установки: `VeryGoodDownloader-<ver>-portable.zip`
    WindowsPortable,
    /// Android: `VeryGoodDownloader-android-<ver>-arm64.apk`
    Android,
}

impl Platform {
    /// Тег релиза относится к этой платформе: ПК — `v1.11.0`, Android — `android-v0.3.0`.
    pub fn tag_matches(self, tag: &str) -> bool {
        let t = tag.trim().to_ascii_lowercase();
        let starts_digit = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_digit());
        match self {
            Platform::Android => t.strip_prefix("android-").is_some_and(|r| starts_digit(r.strip_prefix('v').unwrap_or(r))),
            _ => t.strip_prefix('v').is_some_and(starts_digit),
        }
    }

    /// Точное имя ассета для версии.
    pub fn asset_name(self, version: &str) -> String {
        match self {
            Platform::WindowsSetup => format!("VeryGoodDownloader-Setup-{version}.exe"),
            Platform::WindowsPortable => format!("VeryGoodDownloader-{version}-portable.zip"),
            Platform::Android => format!("VeryGoodDownloader-android-{version}-arm64.apk"),
        }
    }

    /// Похоже на ассет этой платформы (если точного имени нет — например, версия в имени без `-beta.1`).
    fn asset_like(self, name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        match self {
            Platform::WindowsSetup => n.starts_with("verygooddownloader-setup-") && n.ends_with(".exe"),
            Platform::WindowsPortable => n.starts_with("verygooddownloader-") && n.ends_with("-portable.zip"),
            Platform::Android => n.starts_with("verygooddownloader-android-") && n.ends_with("arm64.apk"),
        }
    }

    fn ua(self) -> &'static str {
        match self {
            Platform::Android => "VeryGoodDownloader-Android",
            _ => "VeryGoodDownloader",
        }
    }
}

// ---------- версия ----------

/// Семантическая версия: `1.11.0`, `1.12.0-beta.1`. Из тега: `v1.11.0`, `android-v0.3.0`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// предварительная версия: `beta.1`, `rc.2`; пусто — обычная
    pub pre: String,
}

impl Version {
    /// `1.11.0`, `v1.11`, `android-v0.3.0`, `1.12.0-beta.1`, `1.12.0+build5` (сборка отбрасывается).
    pub fn parse(s: &str) -> Option<Version> {
        let mut t = s.trim();
        if t.len() >= 8 && t[..8].eq_ignore_ascii_case("android-") {
            t = &t[8..];
        }
        t = t.strip_prefix(['v', 'V']).unwrap_or(t);
        let t = t.split('+').next().unwrap_or(t);
        let (nums, pre) = match t.split_once('-') {
            Some((_, "")) => return None,
            Some((n, p)) => (n, p),
            None => (t, ""),
        };
        let parts: Vec<&str> = nums.split('.').collect();
        if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        let n = |i: usize| parts.get(i).map_or(Some(0), |p| p.parse::<u64>().ok());
        if !pre.is_empty() && pre.split('.').any(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
            return None;
        }
        Some(Version { major: n(0)?, minor: n(1)?, patch: n(2)?, pre: pre.to_string() })
    }

    pub fn is_pre(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(o.major, o.minor, o.patch)).then_with(|| cmp_pre(&self.pre, &o.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Правила semver: обычная версия старше предварительной; `beta.2` < `beta.10`; числа младше слов.
fn cmp_pre(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

// ---------- релиз ----------

/// Найденное обновление.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Release {
    /// `1.12.0` (без `v`)
    pub version: String,
    /// тег на GitHub: `v1.12.0`, `android-v0.4.0`
    pub tag: String,
    /// заголовок релиза
    pub title: String,
    /// что нового: тело релиза (markdown)
    pub notes: String,
    /// страница релиза на GitHub
    pub page_url: String,
    /// дата публикации (ISO 8601)
    pub published: String,
    pub prerelease: bool,
    /// файл для скачивания
    pub asset_name: String,
    pub asset_url: String,
    /// размер, байты
    pub size: u64,
    /// SHA-256 ассета (64 символа, строчные); пусто — пока не узнали
    pub sha256: String,
    /// ассет `<имя>.sha256`, если GitHub не прислал `digest`
    pub sha256_url: String,
}

impl Release {
    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.version)
    }
}

/// Строка из 64 шестнадцатеричных символов → строчными.
fn norm_hex(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
}

/// `"sha256:abc…"` из поля `digest` ассета.
fn from_digest(d: &str) -> Option<String> {
    let (alg, hex) = d.split_once(':')?;
    if !alg.trim().eq_ignore_ascii_case("sha256") {
        return None;
    }
    norm_hex(hex)
}

/// Содержимое файла `.sha256`: `abc…  имя` (как у sha256sum / Get-FileHash) или просто хэш.
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let t = text.trim_start_matches('\u{feff}');
    t.split_whitespace().find_map(|w| norm_hex(w.trim_start_matches('*')))
}

/// Все релизы платформы из ответа `GET /repos/…/releases` (черновики и релизы без ассета пропускаются).
pub fn parse_releases(json: &Value, platform: Platform) -> Vec<Release> {
    let Some(list) = json.as_array() else { return Vec::new() };
    let mut out = Vec::new();
    for r in list {
        let tag = r["tag_name"].as_str().unwrap_or("");
        if r["draft"].as_bool().unwrap_or(false) || !platform.tag_matches(tag) {
            continue;
        }
        let Some(ver) = Version::parse(tag) else { continue };
        let version = ver.to_string();
        let empty = Vec::new();
        let assets = r["assets"].as_array().unwrap_or(&empty);
        let name_of = |a: &Value| a["name"].as_str().unwrap_or("").to_string();
        let exact = platform.asset_name(&version);
        let asset = assets
            .iter()
            .find(|a| name_of(a).eq_ignore_ascii_case(&exact))
            .or_else(|| assets.iter().find(|a| platform.asset_like(&name_of(a))));
        let Some(a) = asset else { continue };
        let asset_name = name_of(a);
        let sum_name = format!("{asset_name}.sha256").to_ascii_lowercase();
        let sha256_url = assets
            .iter()
            .find(|x| name_of(x).to_ascii_lowercase() == sum_name)
            .and_then(|x| x["browser_download_url"].as_str())
            .unwrap_or("")
            .to_string();
        out.push(Release {
            version,
            tag: tag.to_string(),
            title: r["name"].as_str().unwrap_or("").to_string(),
            notes: r["body"].as_str().unwrap_or("").replace("\r\n", "\n"),
            page_url: r["html_url"].as_str().unwrap_or("").to_string(),
            published: r["published_at"].as_str().unwrap_or("").to_string(),
            prerelease: r["prerelease"].as_bool().unwrap_or(false),
            asset_url: a["browser_download_url"].as_str().unwrap_or("").to_string(),
            size: a["size"].as_u64().unwrap_or(0),
            sha256: a["digest"].as_str().and_then(from_digest).unwrap_or_default(),
            sha256_url,
            asset_name,
        });
    }
    out
}

/// Самый новый релиз новее `current` (предварительные — только при `beta`).
pub fn pick(releases: &[Release], current: &Version, beta: bool) -> Option<Release> {
    releases
        .iter()
        .filter(|r| beta || !r.prerelease)
        .filter_map(|r| r.version().map(|v| (v, r)))
        // предварительная версия, отмеченная в теге (`-beta.1`), — тоже только с «бета-версиями»
        .filter(|(v, _)| beta || !v.is_pre())
        .filter(|(v, _)| v > current)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r.clone())
}

fn user_agent(platform: Platform, current: &Version) -> String {
    format!("{}/{current}", platform.ua())
}

/// Проверить, есть ли обновление. `Ok(None)` — установлена последняя версия.
/// Ошибка сети начинается с «сеть:» (как в `http`). Найденный релиз всегда с контрольной суммой:
/// если её нет ни в `digest`, ни в `.sha256` — ошибка, обновление не предлагается.
pub fn check(current: &Version, platform: Platform, beta: bool) -> Result<Option<Release>, String> {
    check_at(&api_url(), current, platform, beta)
}

fn check_at(url: &str, current: &Version, platform: Platform, beta: bool) -> Result<Option<Release>, String> {
    let ua = user_agent(platform, current);
    let r = crate::http::get(url)
        .ua(&ua)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(25)
        .retries(2)
        .send()?;
    let json: Value = serde_json::from_slice(&r.body).map_err(|_| format!("непонятный ответ от {} (код {})", crate::http::host(&r.url), r.status))?;
    if r.status != 200 || !json.is_array() {
        let msg = json["message"].as_str().unwrap_or("нет списка релизов");
        return Err(format!("github ответил {}: {msg}", r.status));
    }
    let Some(mut rel) = pick(&parse_releases(&json, platform), current, beta) else { return Ok(None) };
    if rel.sha256.is_empty() && !rel.sha256_url.is_empty() {
        let t = crate::http::get(&rel.sha256_url).ua(&ua).timeout(25).retries(2).send()?;
        if t.status == 200 {
            rel.sha256 = parse_sha256_file(&t.text()).unwrap_or_default();
        }
    }
    if rel.sha256.is_empty() {
        return Err(format!("у версии {} нет контрольной суммы SHA-256 — обновление не ставится", rel.version));
    }
    Ok(Some(rel))
}

/// [`check`] для строки версии (`"1.11.0"`, `"0.3.0"`) — удобно из Kotlin.
pub fn check_str(current: &str, platform: Platform, beta: bool) -> Result<Option<Release>, String> {
    let v = Version::parse(current).ok_or_else(|| format!("непонятная версия «{current}»"))?;
    check(&v, platform, beta)
}

// ---------- скачивание ----------

/// SHA-256 файла (строчными).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Файл совпадает с ожидаемым размером (0 — не проверять) и SHA-256.
pub fn verify(path: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let want = norm_hex(sha256).ok_or("нет контрольной суммы SHA-256")?;
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > 0 && len != size {
        return Err(format!("размер не совпал: {len} байт вместо {size}"));
    }
    let got = sha256_file(path).map_err(|e| e.to_string())?;
    if got != want {
        return Err("контрольная сумма не совпала — файл повреждён или подменён".into());
    }
    Ok(())
}

/// Папка для скачанных обновлений по умолчанию: `<данные>/update`.
pub fn dir() -> PathBuf {
    data_dir().join("update")
}

/// Сколько попыток подряд без единого нового байта, прежде чем сдаться.
const MAX_STALLS: u32 = 8;

enum Fail {
    Cancel,
    /// повторять бессмысленно (404, файл не тот)
    Fatal(String),
    /// обрыв, таймаут, 5xx — докачать
    Retry(String),
}

fn agent(timeouts: bool) -> ureq::Agent {
    let mut b = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(10)
        .save_redirect_history(false)
        .proxy(ureq::Proxy::try_from_env());
    if timeouts {
        // Ни общего лимита: большой файл по медленной связи качается долго, — только «связь замерла».
        b = b
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(40)))
            .timeout_per_read(Some(Duration::from_secs(45)));
    }
    b.build().into()
}

/// Скачать ассет релиза в папку `dir` (имя файла — `asset_name`) с проверкой размера и SHA-256.
///
/// Обрыв (VPN, Wi-Fi) не страшен: недокачанное лежит в `<имя>.part` и докачивается запросом `Range`
/// (в этом же вызове — до [`MAX_STALLS`] попыток подряд без прогресса, и в следующем вызове).
/// Уже скачанный и проверенный файл повторно не качается.
///
/// `on_progress(скачано, всего)` вызывается по ходу; вернула `false` — отмена (ошибка «отменено»,
/// `.part` остаётся для докачки).
pub fn download(rel: &Release, dir: &Path, on_progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<PathBuf, String> {
    download_impl(rel, dir, debug_url(), on_progress)
}

fn download_impl(rel: &Release, dir: &Path, allow_http: bool, on_progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<PathBuf, String> {
    let sha = norm_hex(&rel.sha256).ok_or("нет контрольной суммы SHA-256 — обновление не ставится")?;
    let name = rel.asset_name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return Err(format!("странное имя файла обновления «{name}»"));
    }
    if !(rel.asset_url.starts_with("https://") || (allow_http && rel.asset_url.starts_with("http://"))) {
        return Err("ссылка на обновление не https".into());
    }
    fs::create_dir_all(dir).map_err(|e| format!("папка {}: {e}", dir.display()))?;
    let path = dir.join(name);
    if path.is_file() && verify(&path, rel.size, &sha).is_ok() {
        on_progress(rel.size, rel.size);
        return Ok(path);
    }
    let part = dir.join(format!("{name}.part"));
    let agent = agent(true);
    let ua = format!("VeryGoodDownloader/{}", crate::appinfo::APP_VERSION);
    let mut stalls = 0u32;
    loop {
        let before = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        match fetch(&agent, &ua, rel, &part, on_progress) {
            Ok(()) => break,
            Err(Fail::Cancel) => return Err("отменено".into()),
            Err(Fail::Fatal(e)) => return Err(e),
            Err(Fail::Retry(e)) => {
                let after = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                stalls = if after > before { 0 } else { stalls + 1 };
                if stalls >= MAX_STALLS {
                    return Err(e);
                }
                // 1, 2, 4… до 30 с; сразу после обрыва с прогрессом — секунда
                std::thread::sleep(Duration::from_secs((1u64 << stalls.min(5)).min(30)));
                if !on_progress(after, rel.size) {
                    return Err("отменено".into());
                }
            }
        }
    }
    if let Err(e) = verify(&part, rel.size, &sha) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    let _ = fs::remove_file(&path);
    fs::rename(&part, &path).map_err(|e| format!("не удалось сохранить {}: {e}", path.display()))?;
    Ok(path)
}

/// Одна попытка: с места, где оборвалось (`Range`), до конца.
fn fetch(agent: &ureq::Agent, ua: &str, rel: &Release, part: &Path, on_progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<(), Fail> {
    let mut have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if rel.size > 0 && have > rel.size {
        let _ = fs::remove_file(part);
        have = 0;
    }
    if rel.size > 0 && have == rel.size {
        return Ok(());
    }
    let mut req = agent.get(&rel.asset_url).header("User-Agent", ua).header("Accept", "application/octet-stream");
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let mut resp = req.call().map_err(|e| Fail::Retry(format!("сеть: {e}")))?;
    let code = resp.status().as_u16();
    let append = match code {
        206 if have > 0 => true,
        200 => false,
        // «такого диапазона нет»: .part испорчен или уже полный — начать заново
        416 => {
            let _ = fs::remove_file(part);
            return Err(Fail::Retry("сервер не принял докачку — начинаю заново".into()));
        }
        408 | 425 | 429 | 500..=599 => return Err(Fail::Retry(format!("сервер ответил {code}"))),
        _ => return Err(Fail::Fatal(format!("сервер ответил {code} на {}", crate::http::host(&rel.asset_url)))),
    };
    let mut f = if append {
        fs::OpenOptions::new().append(true).open(part)
    } else {
        have = 0;
        fs::File::create(part)
    }
    .map_err(|e| Fail::Fatal(format!("{}: {e}", part.display())))?;
    let total = if rel.size > 0 { rel.size } else { have + resp.body().content_length().unwrap_or(0) };
    if !on_progress(have, total) {
        return Err(Fail::Cancel);
    }
    let mut r = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = f.flush();
                return Err(Fail::Retry(format!("сеть: обрыв на {} из {} МБ ({e})", have >> 20, total >> 20)));
            }
        };
        f.write_all(&buf[..n]).map_err(|e| Fail::Fatal(format!("запись {}: {e}", part.display())))?;
        have += n as u64;
        if !on_progress(have, total) {
            let _ = f.flush();
            return Err(Fail::Cancel);
        }
    }
    f.flush().map_err(|e| Fail::Fatal(e.to_string()))?;
    if rel.size > 0 && have < rel.size {
        return Err(Fail::Retry(format!("сеть: соединение закрылось на {} из {} МБ", have >> 20, rel.size >> 20)));
    }
    Ok(())
}

// ---------- настройки обновлений ----------

/// «Проверять автоматически», «бета-версии», когда проверяли, какую версию пропустить.
/// Хранится в `<данные>/update.json` (не в settings.json — у Android свои настройки поверх ядра).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// проверять при запуске (не чаще раза в 6 ч)
    pub auto: bool,
    /// предлагать предварительные версии
    pub beta: bool,
    /// последняя проверка (unix, секунды)
    pub last_check: u64,
    /// «пропустить эту версию»: `1.12.0`
    pub skip: String,
    /// что нашла последняя проверка — показать плашку, не дожидаясь следующей проверки
    pub known: Option<Release>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { auto: true, beta: false, last_check: 0, skip: String::new(), known: None }
    }
}

impl Prefs {
    pub fn file() -> PathBuf {
        data_dir().join("update.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::file())
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        self.save_to(&Self::file());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(p) = path.parent() {
            let _ = fs::create_dir_all(p);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, t);
        }
    }

    /// Пора проверять автоматически (прошло 6 ч; часы перевели назад — тоже пора).
    pub fn due(&self, now: u64) -> bool {
        self.auto && (now < self.last_check || now - self.last_check >= CHECK_EVERY)
    }

    /// Пользователь попросил пропустить эту версию.
    pub fn skipped(&self, r: &Release) -> bool {
        !self.skip.is_empty() && self.skip == r.version
    }

    /// Запомнить результат проверки.
    pub fn record(&mut self, now: u64, found: Option<&Release>) {
        self.last_check = now;
        self.known = found.cloned();
    }

    /// Обновление, которое стоит показать без новой проверки: найдено раньше, новее `current`,
    /// не пропущено и подходит по «бета-версиям».
    pub fn pending(&self, current: &Version) -> Option<&Release> {
        self.known.as_ref().filter(|r| {
            !self.skipped(r) && (self.beta || !r.prerelease) && r.version().is_some_and(|v| &v > current && (self.beta || !v.is_pre()))
        })
    }
}

/// Сейчас, unix-секунды.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn version_order() {
        assert!(v("1.9.0") < v("1.10.0"));
        assert!(v("1.10.0") < v("1.11.0"));
        assert!(v("1.11.0") < v("1.11.1"));
        assert!(v("1.11.9") < v("2.0.0"));
        assert_eq!(v("v1.11.0"), v("1.11.0"));
        assert_eq!(v("V1.11"), v("1.11.0"));
        assert_eq!(v("android-v0.3.0"), v("0.3.0"));
        assert_eq!(v("android-0.3.0"), v("0.3.0"));
        assert_eq!(v("1.11.0+build.7"), v("1.11.0"));
        assert!(v("1.12.0-beta.1") < v("1.12.0"));
        assert!(v("1.12.0-beta.2") < v("1.12.0-beta.10"));
        assert!(v("1.12.0-alpha") < v("1.12.0-beta"));
        assert!(v("1.12.0-beta") < v("1.12.0-beta.1"));
        assert!(v("1.12.0-1") < v("1.12.0-alpha"));
        assert!(v("1.11.0") < v("1.12.0-beta.1"));
        assert_eq!(v("1.12.0-rc.1").to_string(), "1.12.0-rc.1");
        assert_eq!(v("v2").to_string(), "2.0.0");
        for bad in ["", "v", "1..2", "1.2.3.4", "x1.2", "1.2.a", "1.2.3-", "1.2.3-a..b", "latest"] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn tags_per_platform() {
        assert!(Platform::WindowsSetup.tag_matches("v1.11.0"));
        assert!(Platform::WindowsPortable.tag_matches("V1.11.0"));
        assert!(!Platform::WindowsSetup.tag_matches("android-v0.3.0"));
        assert!(!Platform::WindowsSetup.tag_matches("very-good"));
        assert!(Platform::Android.tag_matches("android-v0.3.0"));
        assert!(Platform::Android.tag_matches("android-0.3.0"));
        assert!(!Platform::Android.tag_matches("v1.11.0"));
    }

    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const H2: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    /// Ответ GitHub API в сокращённом виде: ПК 1.10.0/1.11.0/1.12.0-beta.1 (prerelease),
    /// черновик 9.9.9, Android 0.3.0/0.4.0, 1.11.1 без digest, но с .sha256.
    fn fixture() -> Value {
        serde_json::from_str(&format!(
            r#"[
          {{"tag_name":"v9.9.9","draft":true,"prerelease":false,"name":"черновик","body":"",
            "assets":[{{"name":"VeryGoodDownloader-Setup-9.9.9.exe","size":1,"browser_download_url":"https://x/s999.exe","digest":"sha256:{H1}"}}]}},
          {{"tag_name":"android-v0.4.0","draft":false,"prerelease":false,"name":"Android 0.4.0","body":"андроид",
            "html_url":"https://github.com/Vivegass/very-good-downloader/releases/tag/android-v0.4.0",
            "assets":[{{"name":"VeryGoodDownloader-android-0.4.0-arm64.apk","size":30000000,"browser_download_url":"https://x/a040.apk","digest":"sha256:{H2}"}}]}},
          {{"tag_name":"v1.12.0-beta.1","draft":false,"prerelease":true,"name":"1.12 бета","body":"бета",
            "assets":[{{"name":"VeryGoodDownloader-Setup-1.12.0-beta.1.exe","size":84000000,"browser_download_url":"https://x/s112b.exe","digest":"sha256:{H1}"}},
                      {{"name":"VeryGoodDownloader-1.12.0-beta.1-portable.zip","size":108000000,"browser_download_url":"https://x/p112b.zip","digest":"sha256:{H2}"}}]}},
          {{"tag_name":"v1.11.1","draft":false,"prerelease":false,"name":"1.11.1","body":"- исправления\r\n- обновление",
            "html_url":"https://github.com/Vivegass/very-good-downloader/releases/tag/v1.11.1","published_at":"2026-11-10T10:00:00Z",
            "assets":[{{"name":"VeryGoodDownloader-Setup-1.11.1.exe","size":83569896,"browser_download_url":"https://x/s1111.exe","digest":null}},
                      {{"name":"VeryGoodDownloader-Setup-1.11.1.exe.sha256","size":100,"browser_download_url":"https://x/s1111.exe.sha256"}},
                      {{"name":"VeryGoodDownloader-1.11.1-portable.zip","size":107249787,"browser_download_url":"https://x/p1111.zip","digest":"sha256:{H2}"}}]}},
          {{"tag_name":"v1.11.0","draft":false,"prerelease":false,"name":"1.11.0","body":"",
            "assets":[{{"name":"VeryGoodDownloader-Setup-1.11.0.exe","size":5,"browser_download_url":"https://x/s1110.exe","digest":"sha256:{H1}"}}]}},
          {{"tag_name":"android-v0.3.0","draft":false,"prerelease":false,"name":"Android 0.3.0","body":"",
            "assets":[{{"name":"VeryGoodDownloader-android-0.3.0-debug-arm64.apk","size":3,"browser_download_url":"https://x/a030.apk","digest":"sha256:{H1}"}}]}},
          {{"tag_name":"v1.10.0","draft":false,"prerelease":false,"name":"1.10.0","body":"",
            "assets":[{{"name":"notes.txt","size":5,"browser_download_url":"https://x/n.txt"}}]}}
        ]"#
        ))
        .unwrap()
    }

    #[test]
    fn parse_api_response_setup() {
        let all = parse_releases(&fixture(), Platform::WindowsSetup);
        let tags: Vec<&str> = all.iter().map(|r| r.tag.as_str()).collect();
        // черновик, Android и релиз без установщика пропущены
        assert_eq!(tags, ["v1.12.0-beta.1", "v1.11.1", "v1.11.0"]);
        let r = &all[1];
        assert_eq!(r.version, "1.11.1");
        assert_eq!(r.asset_name, "VeryGoodDownloader-Setup-1.11.1.exe");
        assert_eq!(r.asset_url, "https://x/s1111.exe");
        assert_eq!(r.size, 83569896);
        assert_eq!(r.sha256, "", "digest null — хэш возьмём из .sha256");
        assert_eq!(r.sha256_url, "https://x/s1111.exe.sha256");
        assert_eq!(r.notes, "- исправления\n- обновление");
        assert_eq!(r.published, "2026-11-10T10:00:00Z");
        assert!(all[0].prerelease);
        assert_eq!(all[0].sha256, H1);
    }

    #[test]
    fn pick_newest_respecting_beta() {
        let setup = parse_releases(&fixture(), Platform::WindowsSetup);
        assert_eq!(pick(&setup, &v("1.11.0"), false).unwrap().version, "1.11.1");
        assert_eq!(pick(&setup, &v("1.11.0"), true).unwrap().version, "1.12.0-beta.1");
        assert!(pick(&setup, &v("1.11.1"), false).is_none());
        assert!(pick(&setup, &v("1.12.0"), true).is_none());
        // бета-тег без отметки prerelease на GitHub — всё равно только с «бета-версиями»
        let mut no_flag = setup.clone();
        no_flag[0].prerelease = false;
        assert_eq!(pick(&no_flag, &v("1.11.0"), false).unwrap().version, "1.11.1");
    }

    #[test]
    fn asset_choice_per_platform() {
        let portable = parse_releases(&fixture(), Platform::WindowsPortable);
        let r = pick(&portable, &v("1.11.0"), false).unwrap();
        assert_eq!(r.asset_name, "VeryGoodDownloader-1.11.1-portable.zip");
        assert_eq!(r.sha256, H2.to_ascii_lowercase());
        assert_eq!(pick(&portable, &v("1.11.0"), true).unwrap().asset_name, "VeryGoodDownloader-1.12.0-beta.1-portable.zip");

        let android = parse_releases(&fixture(), Platform::Android);
        assert_eq!(android.len(), 2);
        let a = pick(&android, &v("0.3.0"), false).unwrap();
        assert_eq!((a.version.as_str(), a.asset_name.as_str()), ("0.4.0", "VeryGoodDownloader-android-0.4.0-arm64.apk"));
        // имя не точное (debug), но подходит по шаблону
        assert_eq!(android[1].asset_name, "VeryGoodDownloader-android-0.3.0-debug-arm64.apk");
        assert!(pick(&android, &v("0.4.0"), true).is_none());

        assert_eq!(Platform::WindowsSetup.asset_name("1.11.0"), "VeryGoodDownloader-Setup-1.11.0.exe");
        assert_eq!(Platform::WindowsPortable.asset_name("1.11.0"), "VeryGoodDownloader-1.11.0-portable.zip");
        assert_eq!(Platform::Android.asset_name("0.3.0"), "VeryGoodDownloader-android-0.3.0-arm64.apk");
    }

    #[test]
    fn sha256_sources() {
        assert_eq!(from_digest(&format!("sha256:{H2}")).unwrap(), H2.to_ascii_lowercase());
        assert!(from_digest("sha512:abc").is_none());
        assert!(from_digest("sha256:1234").is_none());
        assert_eq!(parse_sha256_file(&format!("{H2}  VeryGoodDownloader-Setup-1.11.1.exe\n")).unwrap(), H2.to_ascii_lowercase());
        assert_eq!(parse_sha256_file(&format!("\u{feff}{H1}")).unwrap(), H1);
        assert_eq!(parse_sha256_file(&format!("{H1} *file.exe")).unwrap(), H1);
        assert!(parse_sha256_file("нет хэша").is_none());
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vgd-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha_of(data: &[u8]) -> String {
        Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn hash_check() {
        let d = tmp("hash");
        let p = d.join("f.bin");
        fs::write(&p, b"abc").unwrap();
        // sha256("abc") из стандарта
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&p).unwrap(), abc);
        assert!(verify(&p, 3, abc).is_ok());
        assert!(verify(&p, 0, &abc.to_ascii_uppercase()).is_ok());
        assert!(verify(&p, 4, abc).unwrap_err().contains("размер"));
        assert!(verify(&p, 3, H1).unwrap_err().contains("контрольная сумма"));
        assert!(verify(&p, 3, "").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn prefs_throttle_and_skip() {
        let mut p = Prefs::default();
        assert!(p.auto && !p.beta);
        assert!(p.due(1_000_000));
        let rel = Release { version: "1.11.1".into(), ..Default::default() };
        p.record(1_000_000, Some(&rel));
        assert!(!p.due(1_000_000 + 3600));
        assert!(p.due(1_000_000 + CHECK_EVERY));
        assert!(p.due(10), "часы переведены назад");
        assert_eq!(p.pending(&v("1.11.0")).unwrap().version, "1.11.1");
        assert!(p.pending(&v("1.11.1")).is_none(), "уже обновились");
        p.skip = "1.11.1".into();
        assert!(p.skipped(&rel) && p.pending(&v("1.11.0")).is_none());
        p.auto = false;
        assert!(!p.due(u64::MAX));

        let d = tmp("prefs");
        let f = d.join("update.json");
        p.save_to(&f);
        assert_eq!(Prefs::load_from(&f), p);
        fs::write(&f, "{\"beta\":true}").unwrap();
        let q = Prefs::load_from(&f);
        assert!(q.beta && q.auto, "отсутствующие поля — по умолчанию");
        let _ = fs::remove_dir_all(&d);
    }

    /// Сервер файла с поддержкой Range: первый ответ обрывается на половине, дальше — докачка.
    fn serve_file(data: Vec<u8>, cut_first: bool, requests: usize) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/f.exe", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for i in 0..requests {
                let Ok((mut s, _)) = l.accept() else { return };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut from = 0usize;
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        from = v.trim().trim_end_matches('-').parse().unwrap();
                    }
                    head.push_str(&line);
                }
                seen2.lock().unwrap().push(head);
                let body = &data[from..];
                let status = if from > 0 { "206 Partial Content" } else { "200 OK" };
                let resp = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(resp.as_bytes());
                if cut_first && i == 0 {
                    let _ = s.write_all(&body[..body.len() / 2]);
                    let _ = s.flush();
                    drop(s); // обрыв посередине
                    continue;
                }
                let _ = s.write_all(body);
            }
        });
        (url, seen)
    }

    /// http:// разрешён только в режиме отладки (VGD_UPDATE_URL) — в тестах включаем явно.
    fn dl(rel: &Release, dir: &Path, f: &mut dyn FnMut(u64, u64) -> bool) -> Result<PathBuf, String> {
        download_impl(rel, dir, true, f)
    }

    #[test]
    fn download_resumes_after_cut_and_checks_hash() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let (url, seen) = serve_file(data.clone(), true, 2);
        let d = tmp("dl");
        let rel = Release {
            version: "1.11.1".into(),
            asset_name: "VeryGoodDownloader-Setup-1.11.1.exe".into(),
            asset_url: url,
            size: data.len() as u64,
            sha256: sha_of(&data),
            ..Default::default()
        };
        let mut last = (0, 0);
        let p = dl(&rel, &d, &mut |a, b| {
            last = (a, b);
            true
        })
        .unwrap();
        assert_eq!(fs::read(&p).unwrap(), data);
        assert_eq!(last, (data.len() as u64, data.len() as u64));
        assert!(!d.join("VeryGoodDownloader-Setup-1.11.1.exe.part").exists());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[0].to_ascii_lowercase().contains("user-agent: verygooddownloader/"));
        assert!(seen[1].to_ascii_lowercase().contains("range: bytes=150000-"), "{}", seen[1]);
        // второй раз — уже скачано и проверено, сеть не нужна
        let again = dl(&rel, &d, &mut |_, _| true).unwrap();
        assert_eq!(again, p);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn download_rejects_wrong_hash_and_missing_hash() {
        let data = b"not the installer".to_vec();
        let (url, _) = serve_file(data.clone(), false, 1);
        let d = tmp("bad");
        let mut rel = Release {
            version: "1.11.1".into(),
            asset_name: "x.exe".into(),
            asset_url: url,
            size: data.len() as u64,
            sha256: H1.into(),
            ..Default::default()
        };
        let e = dl(&rel, &d, &mut |_, _| true).unwrap_err();
        assert!(e.contains("контрольная сумма"), "{e}");
        assert!(!d.join("x.exe").exists() && !d.join("x.exe.part").exists());
        rel.sha256.clear();
        assert!(download(&rel, &d, &mut |_, _| true).unwrap_err().contains("SHA-256"));
        rel.sha256 = H1.into();
        rel.asset_name = "..\\evil.exe".into();
        assert!(download(&rel, &d, &mut |_, _| true).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn download_cancel_keeps_part() {
        let data = vec![5u8; 200_000];
        let (url, _) = serve_file(data.clone(), false, 1);
        let d = tmp("cancel");
        let rel = Release {
            version: "1.0.0".into(),
            asset_name: "c.zip".into(),
            asset_url: url,
            size: data.len() as u64,
            sha256: sha_of(&data),
            ..Default::default()
        };
        let e = dl(&rel, &d, &mut |a, _| a < 50_000).unwrap_err();
        assert_eq!(e, "отменено");
        assert!(d.join("c.zip.part").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn check_against_mock_api() {
        // мини-сервер API: список релизов и файл .sha256
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let sum = sha_of(b"x");
        let json = fixture().to_string().replace("https://x/s1111.exe.sha256", &format!("{base}/sum"));
        std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((mut s, _)) = l.accept() else { return };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let body = if first.contains("/sum") { format!("{sum}  VeryGoodDownloader-Setup-1.11.1.exe\n") } else { json.clone() };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        let r = check_at(&format!("{base}/releases"), &v("1.11.0"), Platform::WindowsSetup, false).unwrap().unwrap();
        assert_eq!(r.version, "1.11.1");
        assert_eq!(r.sha256, sha_of(b"x"));
    }
}
