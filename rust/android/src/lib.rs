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
    /// загрузка из подписки (её ссылка)
    pub sub_url: Option<String>,
    /// параметры ссылки (JSON JobOpts: from, to, audio_only, split_tracks, preset, over…)
    pub opts_json: String,
    /// превью видео / обложка (ссылка; пусто — ещё не известно)
    pub thumb: String,
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
            sub_url: j.sub_url.clone(),
            opts_json: serde_json::to_string(&j.opts).unwrap_or_default(),
            thumb: j.thumb.clone(),
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
                        if j.state == State::Done {
                            if let Some(url) = &j.sub_url {
                                if subscription_checked(url, &j.title) {
                                    q.set_settings(Settings::load());
                                }
                            }
                        }
                        l.on_finished(JobInfo::from(j));
                    }
                    save_queue(q);
                    l.on_jobs(jobs_of(q));
                }
                // Нить шлёт Idle последним действием и ещё не завершилась — смотрим только на карточки.
                Outcome::Idle => l.on_idle(q.active()),
            }),
        );
        // Очередь прошлого запуска (процесс могли закрыть посреди загрузки) — на паузе, «продолжить всё» докачает.
        let restored = runner.with(|q| {
            let saved = load_queue();
            for s in &saved {
                let id = q.add(s.url.clone(), s.opts.clone(), s.sub_url.clone(), Some(s.title.clone()).filter(|t| !t.is_empty()));
                if let Some(j) = q.jobs.iter_mut().find(|j| j.id == id) {
                    j.thumb = s.thumb.clone();
                }
                q.pause(id);
            }
            saved.len()
        });
        if restored > 0 {
            listener.on_log(format!("Очередь прошлого запуска: {restored} — на паузе, «продолжить всё» докачает."));
        }
        let core = Arc::new(Core { runner, listener });
        core.notify();
        // у восстановленных карточек без названия — название и превью сразу
        let bare: Vec<u64> = core.runner.with(|q| q.jobs.iter().filter(|j| j.title.starts_with("http")).map(|j| j.id).collect());
        if !bare.is_empty() {
            core.clone().quick_info(bare);
        }
        core
    }

    /// Поставить ссылку с параметрами (JSON `JobOpts`: from, to, audio_only, split_tracks, preset, over) и запустить.
    pub fn add_with(self: Arc<Self>, url: String, opts_json: String) -> Result<u64, VgdError> {
        let opts: JobOpts = serde_json::from_str(&opts_json).map_err(|e| fail(format!("параметры: {e}")))?;
        let id = self.runner.with(|q| {
            let id = q.add(url.trim().to_string(), opts, None, None);
            q.start();
            id
        });
        self.notify();
        self.clone().quick_info(vec![id]);
        Ok(id)
    }

    /// Проверить подписки (все или одну): новые видео/треки; при первой проверке — только запомнить, что есть.
    /// Возвращает, сколько подписок поставлено в очередь.
    pub fn check_subscriptions(&self, only_url: Option<String>) -> u32 {
        let subs = Settings::load().subscriptions;
        let n = self.runner.with(|q| {
            let mut n = 0;
            for sub in &subs {
                if only_url.as_ref().is_some_and(|u| u != &sub.url) {
                    continue;
                }
                // уже в очереди — не дублируем
                if q.jobs.iter().any(|j| j.sub_url.as_deref() == Some(&sub.url) && j.state.is_active()) {
                    continue;
                }
                let opts = JobOpts { audio_only: sub.audio_only, max_age_days: sub.max_age_days, seed_only: !sub.seeded, ..Default::default() };
                let title = Some(sub.name.clone()).filter(|n| !n.is_empty());
                q.add(sub.url.clone(), opts, Some(sub.url.clone()), title);
                n += 1;
            }
            if n > 0 {
                q.start();
            }
            n
        });
        self.notify();
        n
    }

    /// Поставить ссылку в очередь и запустить. `audio_only` — только звук.
    pub fn add(self: Arc<Self>, url: String, audio_only: bool) -> u64 {
        let id = self.runner.with(|q| {
            let opts = JobOpts { audio_only, ..Default::default() };
            let id = q.add(url.trim().to_string(), opts, None, None);
            q.start();
            id
        });
        self.notify();
        self.clone().quick_info(vec![id]);
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

    /// Новый порядок карточек после перетаскивания (id сверху вниз).
    pub fn reorder(&self, ids: Vec<u64>) {
        self.runner.with(|q| q.reorder(&ids));
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
    /// Название и превью карточки сразу, не дожидаясь очереди: лёгкий запрос oEmbed (YouTube, SoundCloud, Vimeo,
    /// Spotify…). Пока названия нет — на карточке ссылка. В отдельной нити, ошибки молча.
    fn quick_info(self: Arc<Self>, ids: Vec<u64>) {
        std::thread::spawn(move || {
            for id in ids {
                let Some(url) = self.runner.with(|q| q.job(id).filter(|j| j.title.starts_with("http")).map(|j| j.url.clone())) else { continue };
                let Some((title, thumb)) = oembed(&url) else { continue };
                let changed = self.runner.with(|q| {
                    let Some(j) = q.jobs.iter_mut().find(|j| j.id == id) else { return false };
                    let mut c = false;
                    if j.title.starts_with("http") && !title.is_empty() {
                        j.title = title.clone();
                        c = true;
                    }
                    if j.thumb.is_empty() && !thumb.is_empty() {
                        j.thumb = thumb.clone();
                        c = true;
                    }
                    c
                });
                if changed {
                    self.notify();
                }
            }
        });
    }

    /// Изменение не от нити загрузки (пауза, порядок) — тоже сообщить экрану.
    fn notify(&self) {
        let jobs = self.runner.with(|q| {
            save_queue(q);
            jobs_of(q)
        });
        self.listener.on_jobs(jobs);
    }
}

/// (название, превью) по oEmbed сайта — без yt-dlp, одним коротким запросом.
fn oembed(url: &str) -> Option<(String, String)> {
    let u = url.to_ascii_lowercase();
    let enc: String = url.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let api = if u.contains("youtube.com/") || u.contains("youtu.be/") {
        format!("https://www.youtube.com/oembed?format=json&url={enc}")
    } else if u.contains("soundcloud.com/") {
        format!("https://soundcloud.com/oembed?format=json&url={enc}")
    } else if u.contains("vimeo.com/") {
        format!("https://vimeo.com/api/oembed.json?url={enc}")
    } else if u.contains("open.spotify.com/") {
        format!("https://open.spotify.com/oembed?url={enc}")
    } else {
        return None;
    };
    let v = vgd_core::http::get(&api).timeout(12).retries(1).json().ok()?;
    let title = v["title"].as_str().unwrap_or("").trim().to_string();
    let thumb = v["thumbnail_url"].as_str().unwrap_or("").to_string();
    (!title.is_empty() || !thumb.is_empty()).then_some((title, thumb))
}

// ---------- обновление программы (GitHub Releases, теги android-v…) ----------

use vgd_core::update::{self, Platform, Prefs, Release};

/// Найденное обновление (копия `update::Release` для Kotlin).
#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdateInfo {
    pub version: String,
    pub tag: String,
    pub title: String,
    /// что нового (markdown)
    pub notes: String,
    pub page_url: String,
    pub published: String,
    pub prerelease: bool,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    pub sha256: String,
    pub sha256_url: String,
}

impl From<Release> for UpdateInfo {
    fn from(r: Release) -> Self {
        UpdateInfo {
            version: r.version, tag: r.tag, title: r.title, notes: r.notes, page_url: r.page_url, published: r.published,
            prerelease: r.prerelease, asset_name: r.asset_name, asset_url: r.asset_url, size: r.size, sha256: r.sha256,
            sha256_url: r.sha256_url,
        }
    }
}

impl From<UpdateInfo> for Release {
    fn from(r: UpdateInfo) -> Self {
        Release {
            version: r.version, tag: r.tag, title: r.title, notes: r.notes, page_url: r.page_url, published: r.published,
            prerelease: r.prerelease, asset_name: r.asset_name, asset_url: r.asset_url, size: r.size, sha256: r.sha256,
            sha256_url: r.sha256_url,
        }
    }
}

/// «Проверять автоматически», «бета-версии», пропущенная версия (`<данные>/update.json`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdatePrefs {
    pub auto: bool,
    pub beta: bool,
    pub skip: String,
    pub last_check: u64,
}

#[uniffi::export]
pub fn update_prefs() -> UpdatePrefs {
    let p = Prefs::load();
    UpdatePrefs { auto: p.auto, beta: p.beta, skip: p.skip, last_check: p.last_check }
}

#[uniffi::export]
pub fn set_update_prefs(auto: bool, beta: bool) {
    let mut p = Prefs::load();
    p.auto = auto;
    p.beta = beta;
    p.save();
}

/// «Пропустить эту версию».
#[uniffi::export]
pub fn update_skip(version: String) {
    let mut p = Prefs::load();
    p.skip = version;
    p.save();
}

/// Пора проверять автоматически (включено и прошло 6 ч).
#[uniffi::export]
pub fn update_due() -> bool {
    Prefs::load().due(update::now())
}

/// Проверить релизы `android-v…`. Some — есть новее `current`; результат запоминается (плашка без сети).
#[uniffi::export]
pub fn update_check(current: String) -> Result<Option<UpdateInfo>, VgdError> {
    let mut p = Prefs::load();
    let found = update::check_str(&current, Platform::Android, p.beta).map_err(fail)?;
    p.record(update::now(), found.as_ref());
    p.save();
    Ok(found.map(UpdateInfo::from))
}

/// Найденное раньше и не пропущенное обновление — показать без новой проверки.
#[uniffi::export]
pub fn update_pending(current: String) -> Option<UpdateInfo> {
    let v = update::Version::parse(&current)?;
    Prefs::load().pending(&v).cloned().map(UpdateInfo::from)
}

/// Прогресс скачивания обновления. false — отменить (недокачанное останется для докачки).
#[uniffi::export(with_foreign)]
pub trait UpdateProgress: Send + Sync {
    fn on_progress(&self, done: u64, total: u64) -> bool;
}

/// Скачать APK в `dir` с докачкой и проверкой размера и SHA-256. Возвращает путь к файлу.
#[uniffi::export]
pub fn update_download(info: UpdateInfo, dir: String, progress: Arc<dyn UpdateProgress>) -> Result<String, VgdError> {
    let rel: Release = info.into();
    let p = update::download(&rel, std::path::Path::new(&dir), &mut |d, t| progress.on_progress(d, t)).map_err(fail)?;
    Ok(p.to_string_lossy().into_owned())
}

/// Только для отладки (debug-сборка): адрес списка релизов вместо GitHub (`VGD_UPDATE_URL`, mock-сервер).
#[uniffi::export]
pub fn set_update_url(url: String) {
    // SAFETY: вызывается один раз при запуске, до нитей загрузок и проверки обновлений.
    unsafe { std::env::set_var(update::ENV_URL, url) };
}

// ---------- очередь между запусками ----------

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
struct SavedJob {
    url: String,
    title: String,
    opts: JobOpts,
    sub_url: Option<String>,
    #[serde(default)]
    thumb: String,
}

fn queue_file() -> PathBuf {
    appinfo::data_dir().join("android_queue.json")
}

/// Незаконченные карточки — в файл (Android может закрыть процесс в любой момент).
fn save_queue(q: &vgd_core::queue::Queue) {
    let v: Vec<SavedJob> = q
        .jobs
        .iter()
        .filter(|j| j.state.is_active())
        .map(|j| SavedJob { url: j.url.clone(), title: j.title.clone(), opts: j.opts.clone(), sub_url: j.sub_url.clone(), thumb: j.thumb.clone() })
        .collect();
    if let Ok(t) = serde_json::to_string(&v) {
        let _ = std::fs::write(queue_file(), t);
    }
}

fn load_queue() -> Vec<SavedJob> {
    std::fs::read_to_string(queue_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Подписка проверена: запомнить, что «старое» учтено, время проверки, название. true — настройки изменились.
fn subscription_checked(url: &str, title: &str) -> bool {
    let mut s = Settings::load();
    let Some(sub) = s.subscriptions.iter_mut().find(|x| x.url == url) else { return false };
    sub.seeded = true;
    sub.last_check = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if sub.name.is_empty() && !title.starts_with("http") {
        sub.name = title.to_string();
    }
    s.save();
    true
}

// ---------- «Разложить папку», категории ----------

/// Перенос файла: откуда → куда.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MovePlan {
    pub from: String,
    pub to: String,
}

/// План раскладки уже скачанных файлов в `root` по «Тема/Канал» (ничего не двигает). Долго — не из главной нити.
#[uniffi::export]
pub fn sort_plan(root: String) -> Result<Vec<MovePlan>, VgdError> {
    let plan = vgd_core::downloader::plan_existing(std::path::Path::new(&root), &|_| {}, None).map_err(fail)?;
    Ok(plan
        .into_iter()
        .map(|(a, b)| MovePlan { from: a.to_string_lossy().into_owned(), to: b.to_string_lossy().into_owned() })
        .collect())
}

/// Разложить по плану. Возвращает, сколько файлов перенесено.
#[uniffi::export]
pub fn sort_apply(plan: Vec<MovePlan>) -> u32 {
    let p: Vec<(PathBuf, PathBuf)> = plan.into_iter().map(|m| (PathBuf::from(m.from), PathBuf::from(m.to))).collect();
    vgd_core::sort::apply_plan(&p, &|_| {}) as u32
}

/// Категория раскладки: название и ключевые слова.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Topic {
    pub name: String,
    pub keywords: Vec<String>,
}

#[uniffi::export]
pub fn topics() -> Vec<Topic> {
    vgd_core::sort::load_topics().into_iter().map(|(name, keywords)| Topic { name, keywords }).collect()
}

#[uniffi::export]
pub fn save_topics(topics: Vec<Topic>) -> Result<(), VgdError> {
    let t: vgd_core::sort::Topics = topics
        .into_iter()
        .filter(|t| !t.name.trim().is_empty())
        .map(|t| (t.name.trim().to_string(), t.keywords.into_iter().map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect()))
        .collect();
    vgd_core::sort::save_topics(&t).map_err(fail)
}

#[uniffi::export]
pub fn default_topics() -> Vec<Topic> {
    vgd_core::sort::default_topics().into_iter().map(|(name, keywords)| Topic { name, keywords }).collect()
}

// ---------- Яндекс Музыка: вход кодом ----------

#[derive(Debug, Clone, uniffi::Record)]
pub struct YandexCode {
    pub device_code: String,
    /// код, который пользователь вводит на странице
    pub user_code: String,
    pub url: String,
    pub interval: u64,
    pub expires_in: u64,
}

#[uniffi::export]
pub fn yandex_start() -> Result<YandexCode, VgdError> {
    let d = vgd_core::yandex_acc::device_start().map_err(fail)?;
    Ok(YandexCode { device_code: d.device_code, user_code: d.user_code, url: d.url, interval: d.interval, expires_in: d.expires_in })
}

/// true — вошли (токен сохранён), false — пользователь ещё не подтвердил.
#[uniffi::export]
pub fn yandex_poll(device_code: String) -> Result<bool, VgdError> {
    vgd_core::yandex_acc::device_poll(&device_code).map_err(fail)
}

#[uniffi::export]
pub fn yandex_logged_in() -> bool {
    vgd_core::yandex_acc::logged_in()
}

#[uniffi::export]
pub fn yandex_logout() {
    vgd_core::yandex_acc::logout()
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
    /// треки по id («-2001_123») — ссылка vk.com/audio-2001_123
    fn by_id(&self, ids: String) -> Result<Vec<VkTrack>, VgdError>;
}

fn hits(v: Vec<VkTrack>) -> Vec<vgd_core::vk::VkHit> {
    v.into_iter()
        .map(|t| vgd_core::vk::VkHit { url: t.url, title: t.title, artist: t.artist, duration: t.duration, cover: t.cover })
        .collect()
}

struct VkBridge(Arc<dyn VkSearcher>);

impl vgd_core::vk::VkBackend for VkBridge {
    fn search(&self, q: &str, count: u32) -> Result<Vec<vgd_core::vk::VkHit>, String> {
        self.0.search(q.to_string(), count).map(hits).map_err(|e| e.to_string())
    }
    fn by_id(&self, ids: &str) -> Result<Vec<vgd_core::vk::VkHit>, String> {
        self.0.by_id(ids.to_string()).map(hits).map_err(|e| e.to_string())
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

// ---------- Spotify: весь плейлист со страницы ----------

/// Страница плейлиста open.spotify.com в скрытом WebView (Kotlin) с `spotify_agent_js()`:
/// JSON `{name, total, tracks: [{id, title, artist, album, duration, cover}]}`.
#[uniffi::export(with_foreign)]
pub trait SpotifyPlaylistPage: Send + Sync {
    fn playlist(&self, id: String) -> Result<String, VgdError>;
}

struct SpotifyBridge(Arc<dyn SpotifyPlaylistPage>);

impl vgd_core::music::SpotifyPage for SpotifyBridge {
    fn playlist(&self, id: &str) -> Result<String, String> {
        self.0.playlist(id.to_string()).map_err(|e| e.to_string())
    }
}

#[uniffi::export]
pub fn set_spotify_page(page: Arc<dyn SpotifyPlaylistPage>) {
    vgd_core::music::set_spotify_page(Arc::new(SpotifyBridge(page)));
}

/// Код для страницы плейлиста Spotify (прокрутка и сбор треков).
#[uniffi::export]
pub fn spotify_agent_js() -> String {
    vgd_core::music::SPOTIFY_AGENT_JS.to_string()
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
    fn saved_job_roundtrip() {
        let j = SavedJob { url: "https://a".into(), title: "А".into(), opts: JobOpts { audio_only: true, ..Default::default() }, sub_url: None, thumb: String::new() };
        // файл прошлой версии (без thumb) читается
        let old: Vec<SavedJob> = serde_json::from_str(r#"[{"url":"u","title":"t","opts":{},"sub_url":null}]"#).unwrap();
        assert_eq!(old[0].thumb, "");
        let t = serde_json::to_string(&vec![j.clone()]).unwrap();
        let back: Vec<SavedJob> = serde_json::from_str(&t).unwrap();
        assert_eq!(back, vec![j]);
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

#[cfg(test)]
mod live_update {
    /// `VGD_UPDATE_URL=http://127.0.0.1:8767/releases cargo test -p vgd-android live_update -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mock_download() {
        let found = super::update_check("0.5.0".into()).unwrap().expect("нет обновления");
        println!("{} {} {}", found.version, found.size, found.sha256);
        struct P;
        impl super::UpdateProgress for P {
            fn on_progress(&self, d: u64, t: u64) -> bool {
                if d % (8 << 20) < 65536 {
                    println!("{d}/{t}");
                }
                true
            }
        }
        let dir = std::env::temp_dir().join("vgd-upd-test");
        let r = super::update_download(found, dir.to_string_lossy().into_owned(), std::sync::Arc::new(P));
        println!("{r:?}");
        assert!(r.is_ok());
    }
}
