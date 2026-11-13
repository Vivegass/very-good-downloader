//! libvgd.so для Android-версии: ядро `vgd_core` + обёртка для Kotlin через UniFFI
//! (Kotlin-код генерирует `uniffi-bindgen` из этой библиотеки, см. `android/README.md`).
//!
//! Наружу:
//! - `init(ToolPaths)` — папки приложения, yt-dlp / ffmpeg / QuickJS из youtubedl-android;
//! - настройки JSON (тот же `settings.json`, что на ПК);
//! - `Core` — очередь (`add`, `pause`, `resume`, `move_job`, `cancel`, …) со слушателем `QueueListener`
//!   (изменения карточек, журнал, «загрузка закончилась»);
//! - `set_vk_searcher` — поиск VK, который делает скрытый WebView на Kotlin (`vk_agent_js`);
//! - версия и обновление yt-dlp.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use vgd_core::appinfo::{self, Paths, Tool};
use vgd_core::queue::{Job, Outcome, Runner, State};
use vgd_core::settings::{JobOpts, Settings};

uniffi::setup_scaffolding!();

// ---------- ошибки ----------

#[derive(Debug, uniffi::Error)]
pub enum VgdError {
    Failed { reason: String },
}

impl std::fmt::Display for VgdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VgdError::Failed { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for VgdError {}

fn fail(reason: impl Into<String>) -> VgdError {
    VgdError::Failed { reason: reason.into() }
}

// ---------- запуск ----------

/// Пути, которые знает только Android (их отдаёт youtubedl-android после распаковки).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ToolPaths {
    /// папка данных приложения: settings.json, темы, списки (`filesDir/vgd`)
    pub data_dir: String,
    /// папка загрузок по умолчанию (`Download/Very Good Downloader`)
    pub download_dir: String,
    /// `nativeLibraryDir/libpython.so` — интерпретатор Python
    pub python: String,
    /// zip-версия yt-dlp (`noBackupFilesDir/youtubedl-android/yt-dlp/yt-dlp`)
    pub ytdlp: String,
    /// `nativeLibraryDir/libffmpeg.so`
    pub ffmpeg: String,
    /// `nativeLibraryDir/libqjs.so` — QuickJS для YouTube (пусто — без JS-движка)
    pub quickjs: String,
    /// переменные окружения для Python и ffmpeg: LD_LIBRARY_PATH, PYTHONHOME, SSL_CERT_FILE, HOME, TMPDIR
    pub env: HashMap<String, String>,
    /// сколько ГБ оставлять свободными на телефоне
    pub min_free_gb: f64,
}

/// Собрать `Paths` ядра. yt-dlp = `python yt-dlp --ffmpeg-location … --js-runtimes quickjs:…`.
fn to_paths(p: &ToolPaths) -> Paths {
    let mut prefix = vec![p.ytdlp.clone(), "--ffmpeg-location".into(), p.ffmpeg.clone()];
    if !p.quickjs.is_empty() {
        prefix.push("--js-runtimes".into());
        prefix.push(format!("quickjs:{}", p.quickjs));
    }
    let mut env: Vec<(String, String)> = p.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    env.sort();
    Paths {
        data_dir: Some(PathBuf::from(&p.data_dir)),
        tools: vec![
            ("yt-dlp".into(), Tool { program: PathBuf::from(&p.python), prefix }),
            ("ffmpeg".into(), Tool { program: PathBuf::from(&p.ffmpeg), prefix: Vec::new() }),
        ],
        env,
        min_free_gb: Some(p.min_free_gb),
        default_out: Some(PathBuf::from(&p.download_dir)),
    }
}

/// Задать пути (один раз за жизнь процесса). Первый запуск — настройки для телефона:
/// куки браузера на Android не читаются (`none`), раскладка по папкам «Тема/Канал» включена.
#[uniffi::export]
pub fn init(paths: ToolPaths) -> bool {
    let first = appinfo::init(to_paths(&paths));
    let _ = std::fs::create_dir_all(&paths.data_dir);
    if !Settings::file().exists() {
        let s = Settings { cookies: "none".into(), sort: true, ..Settings::default() };
        s.save();
    }
    first
}

#[uniffi::export]
pub fn version() -> String {
    appinfo::APP_VERSION.to_string()
}

// ---------- настройки ----------

/// Настройки целиком (JSON, как `settings.json` на ПК).
#[uniffi::export]
pub fn settings_json() -> String {
    serde_json::to_string(&Settings::load()).unwrap_or_else(|_| "{}".into())
}

/// Сохранить настройки (JSON; неизвестные поля игнорируются, недостающие — по умолчанию).
#[uniffi::export]
pub fn save_settings_json(json: String) -> Result<(), VgdError> {
    let s: Settings = serde_json::from_str(&json).map_err(|e| fail(format!("настройки: {e}")))?;
    s.save();
    Ok(())
}

// ---------- очередь ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Done,
    Error,
    Cancelled,
}

impl From<State> for JobState {
    fn from(s: State) -> Self {
        match s {
            State::Queued => JobState::Queued,
            State::Running => JobState::Running,
            State::Paused => JobState::Paused,
            State::Done => JobState::Done,
            State::Error => JobState::Error,
            State::Cancelled => JobState::Cancelled,
        }
    }
}

/// Карточка загрузки для экрана и уведомления.
#[derive(Debug, Clone, uniffi::Record)]
pub struct JobInfo {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub stage: String,
    pub item: String,
    pub percent: f32,
    pub speed: String,
    pub eta: String,
    pub message: String,
    pub files: Vec<String>,
    pub state: JobState,
    pub error: String,
    pub notice: String,
    pub audio_only: bool,
}

impl From<&Job> for JobInfo {
    fn from(j: &Job) -> Self {
        JobInfo {
            id: j.id,
            url: j.url.clone(),
            title: j.title.clone(),
            stage: j.stage.clone(),
            item: j.item.clone(),
            percent: j.percent,
            speed: j.speed.clone(),
            eta: j.eta.clone(),
            message: j.message.clone(),
            files: j.files.clone(),
            state: j.state.into(),
            error: j.error.clone(),
            notice: j.notice.clone(),
            audio_only: j.opts.audio_only,
        }
    }
}

/// Слушатель очереди (Kotlin). Вызывается из нити ядра — переходить в главную нить самому.
#[uniffi::export(with_foreign)]
pub trait QueueListener: Send + Sync {
    /// карточки изменились (прогресс, новая, пауза…): весь список по порядку
    fn on_jobs(&self, jobs: Vec<JobInfo>);
    /// строка журнала
    fn on_log(&self, line: String);
    /// загрузка закончилась (готово / ошибка / отмена / пауза); `files` — новые файлы для MediaScanner
    fn on_finished(&self, job: JobInfo);
    /// очередь опустела — службу можно остановить
    fn on_idle(&self, active: bool);
}

/// Очередь загрузок телефона: одна на процесс (её держит фоновая служба).
#[derive(uniffi::Object)]
pub struct Core {
    runner: Runner,
    listener: Arc<dyn QueueListener>,
}

fn jobs_of(q: &vgd_core::queue::Queue) -> Vec<JobInfo> {
    q.jobs.iter().map(JobInfo::from).collect()
}

#[uniffi::export]
impl Core {
    #[uniffi::constructor]
    pub fn new(listener: Arc<dyn QueueListener>) -> Arc<Self> {
        let l = listener.clone();
        let runner = Runner::new(
            Settings::load(),
            Arc::new(move |out: &Outcome, q: &vgd_core::queue::Queue| match out {
                Outcome::Log(m) => {
                    if !m.starts_with("[download]") {
                        l.on_log(m.clone());
                    }
                }
                Outcome::Progress(_) => l.on_jobs(jobs_of(q)),
                Outcome::Finished(id) => {
                    if let Some(j) = q.job(*id) {
                        l.on_finished(JobInfo::from(j));
                    }
                    l.on_jobs(jobs_of(q));
                }
                // Нить шлёт Idle последним действием и ещё не завершилась — смотрим только на карточки.
                Outcome::Idle => l.on_idle(q.active()),
            }),
        );
        Arc::new(Core { runner, listener })
    }

    /// Поставить ссылку в очередь и запустить. `audio_only` — только звук.
    pub fn add(&self, url: String, audio_only: bool) -> u64 {
        let id = self.runner.with(|q| {
            let opts = JobOpts { audio_only, ..Default::default() };
            let id = q.add(url.trim().to_string(), opts, None, None);
            q.start();
            id
        });
        self.notify();
        id
    }

    pub fn pause(&self, id: u64) {
        self.runner.with(|q| q.pause(id));
        self.notify();
    }

    pub fn resume(&self, id: u64) {
        self.runner.with(|q| q.resume(id));
        self.notify();
    }

    /// Всё на паузу (служба получила `onTimeout` — лимит фоновой работы Android 15).
    pub fn pause_all(&self) {
        self.runner.with(|q| q.pause_all());
        self.notify();
    }

    /// Продолжить всё, что на паузе.
    pub fn resume_all(&self) {
        self.runner.with(|q| {
            let ids: Vec<u64> = q.jobs.iter().filter(|j| j.state == State::Paused).map(|j| j.id).collect();
            for id in ids {
                q.resume(id);
            }
        });
        self.notify();
    }

    /// true — отменяется идущая загрузка (закончится через `on_finished`).
    pub fn cancel(&self, id: u64) -> bool {
        let r = self.runner.with(|q| q.cancel(id));
        self.notify();
        r
    }

    /// Сдвинуть вверх (-1) или вниз (+1).
    pub fn move_job(&self, id: u64, dir: i32) {
        self.runner.with(|q| q.move_job(id, dir));
        self.notify();
    }

    pub fn remove(&self, id: u64) {
        self.runner.with(|q| q.remove(id));
        self.notify();
    }

    pub fn retry(&self, id: u64) {
        self.runner.with(|q| q.retry(id));
        self.notify();
    }

    pub fn clear_done(&self) {
        self.runner.with(|q| q.clear_done());
        self.notify();
    }

    /// Остановить всё (выход).
    pub fn stop_all(&self) {
        self.runner.with(|q| q.stop_all());
    }

    pub fn jobs(&self) -> Vec<JobInfo> {
        self.runner.with(|q| jobs_of(q))
    }

    /// Что-то ждёт или качается.
    pub fn active(&self) -> bool {
        self.runner.with(|q| q.active())
    }

    /// Перечитать настройки после их изменения на экране.
    pub fn reload_settings(&self) {
        self.runner.with(|q| q.set_settings(Settings::load()));
    }
}

impl Core {
    /// Изменение не от нити загрузки (пауза, порядок) — тоже сообщить экрану.
    fn notify(&self) {
        let jobs = self.runner.with(|q| jobs_of(q));
        self.listener.on_jobs(jobs);
    }
}

// ---------- yt-dlp ----------

#[uniffi::export]
pub fn ytdlp_version() -> Option<String> {
    vgd_core::downloader::ytdlp_version()
}

/// Обновить yt-dlp (YouTube часто ломает старые версии). Возвращает новую версию.
#[uniffi::export]
pub fn update_ytdlp() -> Result<String, VgdError> {
    vgd_core::downloader::update_ytdlp(&|_| {}).map_err(fail)
}

// ---------- проверка (этап 0) ----------

fn run(args: &[&str], tool_name: &str) -> (bool, String) {
    use vgd_core::appinfo::{command, tool};
    match command(tool(tool_name)).args(args).output() {
        Ok(o) => (
            o.status.success(),
            format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
        ),
        Err(e) => (false, format!("не запустился {tool_name}: {e}")),
    }
}

/// Форматы ролика: (сколько, лучшая высота, есть ли VP9/AV1 1080p+).
fn formats(url: &str, js: bool) -> Result<(usize, u64, bool), String> {
    let mut args = vec!["-J", "--no-playlist", "--skip-download"];
    if !js {
        args.push("--no-js-runtimes");
    }
    args.push(url);
    let (ok, out) = run(&args, "yt-dlp");
    let start = out.find('{').ok_or_else(|| last_line(&out))?;
    let v: serde_json::Value = serde_json::from_str(out[start..].lines().next().unwrap_or("")).map_err(|_| last_line(&out))?;
    if !ok && v.is_null() {
        return Err(last_line(&out));
    }
    let f = v["formats"].as_array().cloned().unwrap_or_default();
    let best = f.iter().filter_map(|x| x["height"].as_u64()).max().unwrap_or(0);
    let hd_vp9 = f.iter().any(|x| {
        x["height"].as_u64().unwrap_or(0) >= 1080
            && x["vcodec"].as_str().is_some_and(|c| c.starts_with("vp9") || c.starts_with("vp09") || c.starts_with("av01"))
    });
    Ok((f.len(), best, hd_vp9))
}

fn last_line(s: &str) -> String {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("нет ответа").chars().take(300).collect()
}

/// Проверка этапа 0 на телефоне: yt-dlp, кодеки и фильтры ffmpeg, форматы YouTube с QuickJS и без.
/// Долго (до минуты) — звать не из главной нити. Каждая строка — в журнал.
#[uniffi::export]
pub fn diagnose(url: String) -> Vec<String> {
    let mut out = Vec::new();
    let (_, v) = run(&["--version"], "yt-dlp");
    out.push(format!("yt-dlp: {}", last_line(&v)));
    let (_, enc) = run(&["-hide_banner", "-encoders"], "ffmpeg");
    let (_, flt) = run(&["-hide_banner", "-filters"], "ffmpeg");
    let has = |text: &str, name: &str| text.split_whitespace().any(|w| w == name);
    for name in ["libx264", "h264_mediacodec", "aac", "libmp3lame", "libopus", "prores_ks"] {
        out.push(format!("ffmpeg кодек {name}: {}", if has(&enc, name) { "есть" } else { "НЕТ" }));
    }
    for name in ["zscale", "tonemap", "scale", "fps"] {
        out.push(format!("ffmpeg фильтр {name}: {}", if has(&flt, name) { "есть" } else { "НЕТ" }));
    }
    for js in [true, false] {
        let label = if js { "с QuickJS" } else { "без JS-движка" };
        match formats(&url, js) {
            Ok((n, h, hd)) => out.push(format!(
                "форматы YouTube {label}: {n}, лучшая высота {h}p, VP9/AV1 1080p+: {}",
                if hd { "есть" } else { "нет" }
            )),
            Err(e) => out.push(format!("форматы YouTube {label}: ошибка — {e}")),
        }
    }
    out.push(format!("свободно в папке загрузок: {:.1} ГБ", vgd_core::downloader::free_gb(&Settings::load().out_dir).unwrap_or(-1.0)));
    out
}

// ---------- VK ----------

/// Трек VK из скрытого WebView.
#[derive(Debug, Clone, uniffi::Record)]
pub struct VkTrack {
    pub url: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<f64>,
    pub cover: Option<String>,
}

/// Поиск в VK: делает скрытый WebView с `vk_agent_js()` (Kotlin).
#[uniffi::export(with_foreign)]
pub trait VkSearcher: Send + Sync {
    fn search(&self, q: String, count: u32) -> Result<Vec<VkTrack>, VgdError>;
    fn logged_in(&self) -> bool;
}

struct VkBridge(Arc<dyn VkSearcher>);

impl vgd_core::vk::VkBackend for VkBridge {
    fn search(&self, q: &str, count: u32) -> Result<Vec<vgd_core::vk::VkHit>, String> {
        self.0
            .search(q.to_string(), count)
            .map(|v| {
                v.into_iter()
                    .map(|t| vgd_core::vk::VkHit { url: t.url, title: t.title, artist: t.artist, duration: t.duration, cover: t.cover })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }
    fn logged_in(&self) -> bool {
        self.0.logged_in()
    }
}

#[uniffi::export]
pub fn set_vk_searcher(searcher: Arc<dyn VkSearcher>) {
    vgd_core::vk::set_backend(Arc::new(VkBridge(searcher)));
}

/// Код для страницы vk.com (тот же, что у WebView2 на ПК).
#[uniffi::export]
pub fn vk_agent_js() -> String {
    vgd_core::vk::AGENT_JS.to_string()
}

// ---------- темы ----------

/// Тема для выбора в настройках: встроенная или файл из папки тем (тот же JSON, что на ПК).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ThemeChoice {
    /// ключ (как `Settings::theme`): "material", "sunset", "file:моя"
    pub key: String,
    pub name: String,
    pub dark: bool,
}

#[uniffi::export]
pub fn theme_list() -> Vec<ThemeChoice> {
    use vgd_core::theme;
    let mut v: Vec<ThemeChoice> = theme::builtins()
        .into_iter()
        .map(|(k, t)| ThemeChoice { key: k.to_string(), name: t.name.clone(), dark: !t.is_light() })
        .collect();
    for (stem, t) in theme::list_custom().0 {
        v.push(ThemeChoice { key: format!("{}{stem}", theme::FILE_PREFIX), name: t.name.clone(), dark: !t.is_light() });
    }
    v
}

/// Роли Compose `ColorScheme` для темы (JSON: {"primary": "#rrggbb", …}).
/// "material" — Material 3 с ПК: светлая или тёмная по системе (`system_dark`).
/// У тем не на M3 роли подбираются из их цветов (акцент → primary, фон → background…).
#[uniffi::export]
pub fn theme_scheme_json(key: String, system_dark: bool) -> String {
    use vgd_core::theme::{self, m3::Scheme};
    let t = if key == "material" || key == "material-light" { theme::material(system_dark) } else { theme::load(&key) };
    let s: Scheme = match &t.m3 {
        Some(m) => m.roles(),
        None => Scheme {
            primary: t.accent,
            on_primary: t.on_accent,
            primary_container: t.selection,
            on_primary_container: t.text,
            inverse_primary: t.accent_hover,
            secondary: t.accent_hover,
            on_secondary: t.on_accent,
            secondary_container: t.pill,
            on_secondary_container: t.text,
            tertiary: t.ok,
            on_tertiary: t.bg,
            tertiary_container: t.ok_bg,
            on_tertiary_container: t.text,
            background: t.bg,
            on_background: t.text,
            surface: t.bg,
            on_surface: t.text,
            surface_variant: t.surface_hi,
            on_surface_variant: t.muted,
            surface_tint: t.accent,
            inverse_surface: t.text,
            inverse_on_surface: t.bg,
            error: t.err,
            on_error: t.bg,
            error_container: t.warn_bg,
            on_error_container: t.text,
            outline: t.border,
            outline_variant: t.divider,
            scrim: theme::Rgb([0, 0, 0]),
            surface_bright: t.surface_hi,
            surface_container: t.surface,
            surface_container_high: t.window,
            surface_container_highest: t.field,
            surface_container_low: t.sidebar,
            surface_container_lowest: t.bg,
            surface_dim: t.bg,
        },
    };
    serde_json::to_string(&s).unwrap_or_else(|_| "{}".into())
}

// ---------- ссылки ----------

/// Найти ссылку в тексте «Поделиться» («Смотри: https://youtu.be/… — классно»).
#[uniffi::export]
pub fn extract_url(text: String) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, '"' | '\'' | '«' | '»' | '(' | ')' | '<' | '>' | ',' | ';')))
        .find(|w| w.starts_with("https://") || w.starts_with("http://"))
        .map(String::from)
}

/// Музыкальная ссылка (Spotify, SoundCloud, YouTube Music, Яндекс…) — для подписи в интерфейсе.
#[uniffi::export]
pub fn is_music_url(url: String) -> bool {
    vgd_core::music::is_music_url(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверка на ПК с настоящими yt-dlp/ffmpeg в PATH: `cargo test -p vgd-android -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_diagnose() {
        let lines = diagnose("https://www.youtube.com/watch?v=jNQXAC9IVRw".into());
        for l in &lines {
            println!("{l}");
        }
        assert!(lines.iter().any(|l| l.starts_with("форматы YouTube с QuickJS")));
    }

    #[test]
    fn share_text_to_url() {
        assert_eq!(extract_url("Смотри: https://youtu.be/abc?si=1 — классно".into()).as_deref(), Some("https://youtu.be/abc?si=1"));
        assert_eq!(extract_url("«https://music.yandex.ru/album/1»".into()).as_deref(), Some("https://music.yandex.ru/album/1"));
        assert_eq!(extract_url("нет ссылки".into()), None);
    }

    #[test]
    fn material_scheme_follows_system() {
        let dark: serde_json::Value = serde_json::from_str(&theme_scheme_json("material".into(), true)).unwrap();
        let light: serde_json::Value = serde_json::from_str(&theme_scheme_json("material".into(), false)).unwrap();
        assert!(dark["primary"].as_str().unwrap().starts_with('#'));
        assert_ne!(dark["surface"], light["surface"]);
        // тема не на M3 — роли из её цветов
        let sunset: serde_json::Value = serde_json::from_str(&theme_scheme_json("sunset".into(), true)).unwrap();
        assert!(sunset["onSurfaceVariant"].is_string());
    }

    #[test]
    fn paths_for_android() {
        let p = to_paths(&ToolPaths {
            data_dir: "/data/user/0/x/files/vgd".into(),
            download_dir: "/storage/emulated/0/Download/Very Good Downloader".into(),
            python: "/lib/libpython.so".into(),
            ytdlp: "/nb/yt-dlp".into(),
            ffmpeg: "/lib/libffmpeg.so".into(),
            quickjs: "/lib/libqjs.so".into(),
            env: HashMap::from([("PYTHONHOME".into(), "/py/usr".into())]),
            min_free_gb: 1.0,
        });
        let yt = &p.tools[0].1;
        assert_eq!(yt.program, PathBuf::from("/lib/libpython.so"));
        assert_eq!(yt.prefix, ["/nb/yt-dlp", "--ffmpeg-location", "/lib/libffmpeg.so", "--js-runtimes", "quickjs:/lib/libqjs.so"]);
        assert_eq!(p.env, [("PYTHONHOME".to_string(), "/py/usr".to_string())]);
    }
}
