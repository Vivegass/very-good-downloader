//! Яндекс Музыка от аккаунта пользователя: вход кодом (OAuth Device Flow — пароль программа не видит),
//! поиск треков и прямая ссылка на файл (`get-file-info`, способ доставки `raw` — обычный файл).
//! С подпиской Плюс — полные треки: mp3 320 / aac и, если есть, flac; без подписки Яндекс отдаёт отрывки.
//! Зашифрованную доставку (`encraw`) программа не берёт и не расшифровывает.
//! По образцу неофициальной библиотеки MarshalX/yandex-music-api.

use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::path::PathBuf;

use crate::appinfo::{command, data_dir};

const OAUTH: &str = "https://oauth.yandex.ru";
const API: &str = "https://api.music.yandex.net";
// Приложение Яндекс Музыки для Android (как в yandex-music-api).
const CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
const CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";
// Подпись get-file-info: ключ и заголовок десктопного приложения для Windows.
const SIGN_KEY: &str = "kzqU4XhfCaY6B6JTHODeq5";
const DESKTOP_CLIENT: &str = "YandexMusicDesktopAppWindows/5.95.0";
const CODECS: [&str; 7] = ["flac", "flac-mp4", "aac-mp4", "he-aac-mp4", "aac", "he-aac", "mp3"];

fn token_file() -> PathBuf {
    data_dir().join("yandex_token.json")
}

pub fn token() -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(token_file()).ok()?).ok()?;
    v["access_token"].as_str().filter(|t| !t.is_empty()).map(String::from)
}

pub fn logged_in() -> bool {
    token().is_some()
}

pub fn logout() {
    let _ = std::fs::remove_file(token_file());
}

fn curl(args: &[&str]) -> Result<Value, String> {
    let o = command("curl.exe")
        .args(["-s", "-S", "-L", "--compressed", "--max-time", "30", "--retry", "2"])
        .args(args)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !o.status.success() {
        return Err(format!("сеть: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    serde_json::from_slice(&o.stdout).map_err(|_| "Яндекс ответил непонятно".to_string())
}

fn form(pairs: &[(&str, &str)]) -> Vec<String> {
    pairs.iter().flat_map(|(k, v)| ["--data-urlencode".to_string(), format!("{k}={v}")]).collect()
}

/// Начало входа: код, который пользователь вводит на странице Яндекса.
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub url: String,
    pub interval: u64,
    pub expires_in: u64,
}

pub fn device_start() -> Result<DeviceCode, String> {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let device_id = format!("{:032x}", t ^ ((std::process::id() as u128) << 64));
    let mut args: Vec<String> = form(&[("client_id", CLIENT_ID), ("device_id", &device_id), ("device_name", "Very Good Downloader")]);
    args.push(format!("{OAUTH}/device/code"));
    let v = curl(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(DeviceCode {
        device_code: v["device_code"].as_str().ok_or_else(|| format!("Яндекс: {}", v["error_description"].as_str().unwrap_or("не дал код")))?.to_string(),
        user_code: v["user_code"].as_str().unwrap_or("").to_string(),
        url: v["verification_url"].as_str().unwrap_or("https://ya.ru/device").to_string(),
        interval: v["interval"].as_u64().unwrap_or(5).max(2),
        expires_in: v["expires_in"].as_u64().unwrap_or(300),
    })
}

/// Опрос: Ok(true) — вошли (токен сохранён), Ok(false) — пользователь ещё не подтвердил.
pub fn device_poll(device_code: &str) -> Result<bool, String> {
    let mut args = form(&[("grant_type", "device_code"), ("code", device_code), ("client_id", CLIENT_ID), ("client_secret", CLIENT_SECRET)]);
    args.push(format!("{OAUTH}/token"));
    let v = curl(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    if let Some(t) = v["access_token"].as_str() {
        let saved = json!({ "access_token": t, "refresh_token": v["refresh_token"], "expires_in": v["expires_in"] });
        std::fs::write(token_file(), saved.to_string()).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    match v["error"].as_str() {
        Some("authorization_pending") | Some("slow_down") => Ok(false),
        Some(_) => Err(v["error_description"].as_str().unwrap_or("вход не удался").to_string()),
        None => Ok(false),
    }
}

/// Войти целиком: показать код (`show`), ждать подтверждения.
pub fn login(show: &dyn Fn(&str, &str)) -> Result<(), String> {
    let d = device_start()?;
    show(&d.user_code, &d.url);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(d.expires_in);
    while std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_secs(d.interval));
        if device_poll(&d.device_code)? {
            return Ok(());
        }
    }
    Err("время на вход вышло — попробуйте ещё раз".into())
}

fn auth_get(url: &str, extra: &[&str]) -> Result<Value, String> {
    let token = token().ok_or("нет входа в Яндекс Музыку")?;
    let auth = format!("Authorization: OAuth {token}");
    let mut args: Vec<&str> = vec!["-H", &auth];
    args.extend_from_slice(extra);
    args.push(url);
    let v = curl(&args)?;
    if v["error"].is_object() || v["error"].is_string() {
        let msg = v["error"]["message"].as_str().or(v["error"].as_str()).unwrap_or("ошибка");
        if msg.contains("session-expired") || msg.contains("Unauthorized") || msg.contains("not-allowed") {
            return Err(format!("Яндекс: вход устарел — войдите снова ({msg})"));
        }
        return Err(format!("Яндекс: {msg}"));
    }
    Ok(v)
}

/// Подпись get-file-info: HMAC-SHA256(ts + id + quality + codecs + transport), base64 без последнего символа.
fn file_info_sign(ts: u64, track_id: &str, quality: &str, transport: &str) -> String {
    let msg = format!("{ts}{track_id}{quality}{}{transport}", CODECS.concat());
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(SIGN_KEY.as_bytes()).expect("ключ любой длины");
    mac.update(msg.as_bytes());
    let mut s = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    s.pop();
    s
}

/// Прямая ссылка на файл трека: (ссылка, кодек). `lossless` — flac, если есть; иначе лучший из остальных.
pub fn file_url(track_id: &str, lossless: bool) -> Result<(String, String), String> {
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let quality = if lossless { "lossless" } else { "hq" };
    let sign = file_info_sign(ts, track_id, quality, "raw");
    let url = format!(
        "{API}/get-file-info?ts={ts}&trackId={track_id}&quality={quality}&codecs={}&transports=raw&sign={}",
        CODECS.join("%2C"),
        enc(&sign)
    );
    let client = format!("X-Yandex-Music-Client: {DESKTOP_CLIENT}");
    let v = auth_get(&url, &["-H", &client])?;
    let info = &v["result"]["downloadInfo"];
    let link = info["url"].as_str().or_else(|| info["urls"].as_array().and_then(|a| a.first()).and_then(Value::as_str));
    if info["transport"].as_str().is_some_and(|t| t != "raw") || info["key"].as_str().is_some_and(|k| !k.is_empty()) {
        return Err("Яндекс отдал только зашифрованный файл — такой не скачиваем".into());
    }
    match link {
        Some(u) => Ok((u.to_string(), info["codec"].as_str().unwrap_or("").to_string())),
        None => Err("Яндекс не дал ссылку на файл (нет подписки Плюс или трек недоступен)".into()),
    }
}

/// Поиск треков: (id, название, исполнитель, длительность в секундах).
pub fn search(q: &str) -> Result<Vec<(String, String, String, Option<f64>)>, String> {
    let v = auth_get(&format!("{API}/search?text={}&type=track&page=0&nocorrect=false", enc(q)), &[])?;
    Ok(v["result"]["tracks"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(6)
        .filter(|t| t["available"].as_bool() != Some(false))
        .filter_map(|t| {
            let id = t["id"].as_str().map(String::from).or_else(|| t["id"].as_i64().map(|i| i.to_string()))?;
            let mut title = t["title"].as_str().unwrap_or("").to_string();
            if let Some(ver) = t["version"].as_str().filter(|x| !x.is_empty()) {
                title = format!("{title} ({ver})");
            }
            let artist = t["artists"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect::<Vec<_>>().join(", ");
            Some((id, title, artist, t["durationMs"].as_f64().map(|ms| ms / 1000.0)))
        })
        .collect())
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Та же подпись, что у Python: base64(hmac_sha256(...))[:-1].
    #[test]
    fn sign_matches_python() {
        // python: hmac.new(b'kzqU4XhfCaY6B6JTHODeq5', b'1700000000123losslessflacflac-mp4aac-mp4he-aac-mp4aache-aacmp3raw', sha256)
        let s = file_info_sign(1_700_000_000, "123", "lossless", "raw");
        assert_eq!(s, "iZyXIDbop0dbtTaOklZtW02559UkGiSPaZJ7VAHF+Cc");
    }
}
