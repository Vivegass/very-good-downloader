//! Окно программы: минималистичный тёмный интерфейс — ссылка, очередь, настройки, журнал.
#![windows_subsystem = "windows"]

use eframe::egui;
use egui::{
    Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, RichText,
    Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use vgd::appinfo::{open_path, reveal, APP_NAME, APP_VERSION};
use vgd::ipc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use vgd::downloader::{plan_existing, update_ytdlp, ytdlp_version, CancelHandle, DlError, Downloader, Progress};
use vgd::settings::{JobOpts, Settings, Subscription, BROWSERS};
use vgd::sort::{apply_plan, default_topics, load_topics, save_topics, Topics};

// ---------- палитра ----------

const BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const SIDEBAR: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0b);
const SURFACE: Color32 = Color32::from_rgb(0x15, 0x15, 0x15);
const SURFACE_HI: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
const MUTED: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x8c);
const FAINT: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
const ACCENT: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);
const ON_ACCENT: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const OK: Color32 = Color32::from_rgb(0x74, 0xd6, 0x80);
const ERR: Color32 = Color32::from_rgb(0xf0, 0x6e, 0x6e);
const WARN: Color32 = Color32::from_rgb(0xe8, 0xb4, 0x4c);

const COLUMN: f32 = 640.0;

// Значки из шрифта Segoe MDL2 Assets (есть в каждой Windows 10/11).
const I_DOWNLOAD: char = '\u{E896}';
const I_SETTINGS: char = '\u{E713}';
const I_LOG: char = '\u{E8A5}';
const I_LINK: char = '\u{E71B}';
const I_PASTE: char = '\u{E77F}';
const I_FOLDER: char = '\u{E8B7}';
const I_CLOSE: char = '\u{E711}';
const I_RETRY: char = '\u{E72C}';
const I_SUBS: char = '\u{E728}';
const I_PAUSE: char = '\u{E769}';
const I_PLAY: char = '\u{E768}';
const I_UP: char = '\u{E70E}';
const I_DOWN: char = '\u{E70D}';

fn icons() -> FontFamily {
    FontFamily::Name("icons".into())
}

// ---------- названия ----------

fn browser_name(b: &str) -> &'static str {
    match b {
        "firefox" => "firefox",
        "chrome" => "chrome",
        "edge" => "edge",
        "brave" => "brave",
        "opera" => "opera",
        "vivaldi" => "vivaldi",
        "chromium" => "chromium",
        "file" => "файл…",
        _ => "нет",
    }
}

fn height_short(h: u32) -> &'static str {
    match h {
        2160 => "4k",
        1440 => "1440p",
        1080 => "1080p",
        720 => "720p",
        480 => "480p",
        _ => "макс",
    }
}

fn stage_name(s: &str) -> &'static str {
    match s {
        "video" => "видео",
        "audio" => "звук",
        "merge" => "склейка",
        "edit" => "перекодирование",
        "subs" => "субтитры",
        "done" => "готово",
        "error" => "ошибка",
        "wait" => "ожидание",
        "cancel" => "отменено",
        "queued" => "в очереди",
        "paused" => "на паузе",
        _ => "подготовка",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Download,
    Settings,
    Subs,
    Log,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Queued,
    Running,
    Paused,
    Done,
    Error,
    Cancelled,
}

struct Job {
    id: u64,
    url: String,
    title: String,
    stage: String,
    item: String,
    percent: f32,
    speed: String,
    eta: String,
    message: String,
    files: Vec<String>,
    state: State,
    error: String,
    /// важная подсказка от загрузчика (например, про 100 треков Spotify)
    notice: String,
    /// фрагмент / только звук / параметры подписки
    opts: JobOpts,
    /// загрузка из подписки (её ссылка)
    sub_url: Option<String>,
}

enum Event {
    Progress(u64, Progress),
    Log(String),
    Finish(u64, State, Vec<String>, Option<String>),
    Idle,
    Status(String),
    SortPlan(Vec<(PathBuf, PathBuf)>),
    YtdlpVersion(String),
    /// ссылки из браузера или второго запуска программы
    External(Vec<String>),
    /// установка кнопок в Spotify закончилась: успех / текст ошибки
    SpotifyDone(Result<(), String>),
    /// вход в Яндекс: код и страница (Some) или вход закончился (None)
    YandexCode(Option<(String, String)>),
}

/// Общее между окном и рабочим потоком.
#[derive(Default)]
struct Shared {
    /// ждут загрузки, в порядке карточек (пересобирается из `App::jobs` в `sync_queue`)
    queue: VecDeque<(u64, String, JobOpts)>,
    /// что качается сейчас: основная нить и нить для небольшой музыки
    current: Vec<(u64, CancelHandle)>,
    settings: Settings,
    stop_all: bool,
}

enum Action {
    Cancel(u64),
    Pause(u64),
    Resume(u64),
    /// сдвинуть карточку вверх (-1) или вниз (+1)
    Move(u64, i32),
    /// открыть/закрыть настройки ссылки
    EditOpts(u64),
    Remove(u64),
    Retry(u64),
    Reveal(String),
}

struct App {
    page: Page,
    s: Settings,
    urls: String,
    jobs: Vec<Job>,
    next_id: u64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    shared: Arc<Mutex<Shared>>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// вторая нить: небольшая музыка, пока основная занята длинной загрузкой
    side_worker: Option<std::thread::JoinHandle<()>>,
    /// загрузки, которые останавливаем ради паузы (а не отмены)
    pausing: std::collections::HashSet<u64>,
    /// карточка, у которой открыты свои настройки
    edit_job: Option<u64>,
    /// поле «название нового пресета»
    preset_new: String,
    /// вход в Яндекс идёт: код и страница для ввода
    ya_code: Option<(String, String)>,
    log: Vec<String>,
    status: String,
    sort_plan: Option<Vec<(PathBuf, PathBuf)>>,
    sort_root: PathBuf,
    confirm_exit: bool,
    exiting: bool,
    side_task: bool,
    ytdlp: String,
    logo: Option<egui::TextureHandle>,
    ctx: egui::Context,
    /// параметры для следующих ссылок: фрагмент, только звук
    next_opts: JobOpts,
    show_opts: bool,
    /// буфер обмена: последняя увиденная ссылка и предложение её скачать
    clip_last: String,
    clip_offer: Option<String>,
    clip_at: f64,
    /// поле «добавить подписку»
    sub_new: String,
    subs_started: bool,
    /// значок в трее и его меню
    _tray: Option<TrayIcon>,
    tray_rx: Receiver<TrayIconEvent>,
    menu_rx: Receiver<MenuEvent>,
    tray_open: MenuId,
    tray_quit: MenuId,
    hidden: bool,
    /// новый масштаб интерфейса — применяется в начале следующего кадра, а не посреди отрисовки
    pending_zoom: Option<f32>,
    /// после «установить в браузер»: в какой браузер ставим (показываем шаги)
    ext_steps: Option<&'static str>,
    /// открыто из установщика (`--page=extension`): сразу предложить выбрать браузер
    ext_wizard: bool,
    /// редактор категорий: (название до правки, название, ключевые слова через запятую)
    topics_edit: Option<Vec<(String, String, String)>>,
    /// главный экран: раскрыт список папок загрузок
    show_folders: bool,
    /// ссылки из командной строки («Very Good Downloader.exe <ссылка>») — запускаются сразу
    start_urls: Vec<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        // Масштаб только из настроек: Ctrl+колесо/Ctrl+= в egui отключены, чтобы не путались с сохранённым.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        // Приём ссылок из расширения браузера, пока окно открыто.
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            ipc::serve(move |urls| {
                let _ = tx.send(Event::External(urls));
                ctx.request_repaint();
            });
        }
        let s = Settings::load();
        cc.egui_ctx.set_zoom_factor(s.ui_scale.clamp(0.5, 3.0));
        let logo = eframe::icon_data::from_png_bytes(include_bytes!("../../app.png")).ok().map(|ic| {
            let img = egui::ColorImage::from_rgba_unmultiplied([ic.width as usize, ic.height as usize], &ic.rgba);
            cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
        });
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::YtdlpVersion(ytdlp_version().unwrap_or_else(|| "не найден".into())));
                ctx.request_repaint();
            });
        }
        // Spotify обновился и кнопки «скачать» пропали — вернуть (только пока Spotify закрыт).
        if s.spotify_app {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let ltx = tx.clone();
                if let Some(Err(e)) = vgd::spotify_app::repair(&|m| {
                    let _ = ltx.send(Event::Log(m));
                }) {
                    let _ = tx.send(Event::Log(format!("Ошибка: Spotify: {e}")));
                }
            });
        }
        let tray = make_tray(&cc.egui_ctx);
        // «--page=settings» / «--page=log» — открыть сразу нужную страницу.
        // «--page=extension» — из установщика: настройки с выбором браузера для расширения.
        let page_arg = std::env::args().find_map(|a| a.strip_prefix("--page=").map(String::from));
        let ext_wizard = page_arg.as_deref() == Some("extension");
        let page = match page_arg.as_deref() {
            Some("settings") | Some("extension") => Page::Settings,
            Some("log") => Page::Log,
            Some("subs") => Page::Subs,
            _ => Page::Download,
        };
        Self {
            page,
            shared: Arc::new(Mutex::new(Shared { settings: s.clone(), ..Default::default() })),
            s,
            urls: String::new(),
            jobs: Vec::new(),
            next_id: 1,
            tx,
            rx,
            worker: None,
            side_worker: None,
            pausing: Default::default(),
            edit_job: None,
            preset_new: String::new(),
            ya_code: None,
            log: Vec::new(),
            status: String::new(),
            sort_plan: None,
            sort_root: PathBuf::new(),
            confirm_exit: false,
            exiting: false,
            side_task: false,
            ytdlp: "…".into(),
            logo,
            ctx: cc.egui_ctx.clone(),
            next_opts: JobOpts::default(),
            show_opts: false,
            clip_last: String::new(),
            clip_offer: None,
            clip_at: 0.0,
            sub_new: String::new(),
            subs_started: false,
            _tray: tray.0,
            tray_rx: tray.1,
            menu_rx: tray.2,
            tray_open: tray.3,
            tray_quit: tray.4,
            hidden: false,
            pending_zoom: None,
            ext_steps: None,
            ext_wizard,
            topics_edit: None,
            show_folders: false,
            start_urls: ipc::launch_urls(std::env::args().skip(1)),
        }
    }

    /// Идёт загрузка (в любой из двух нитей).
    fn busy(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished()) || self.side_worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    fn add_log(&mut self, msg: String) {
        if msg.starts_with("[download]") {
            return;
        }
        self.log.push(msg);
        if self.log.len() > 3000 {
            let extra = self.log.len() - 3000;
            self.log.drain(..extra);
        }
    }

    fn save_settings(&mut self) {
        self.s.out_dir = self.s.out_dir.trim().to_string();
        if self.s.out_dir.is_empty() {
            self.s.out_dir = vgd::settings::default_out();
        }
        self.s.save();
        self.shared.lock().unwrap().settings = self.s.clone();
    }

    fn paste(&mut self) {
        if let Some(text) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()) {
            let text = text.trim();
            if !text.is_empty() {
                if !self.urls.trim().is_empty() && !self.urls.ends_with('\n') {
                    self.urls.push('\n');
                }
                self.urls.push_str(text);
            }
        }
    }

    fn add_and_start(&mut self, ctx: &egui::Context) {
        let mut lines: Vec<String> =
            self.urls.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
        if lines.is_empty() {
            self.paste();
            lines = self.urls.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
        }
        if lines.is_empty() {
            return;
        }
        self.save_settings();
        let opts = self.next_opts.clone();
        for url in lines {
            self.enqueue(url, opts.clone(), None);
        }
        self.urls.clear();
        self.page = Page::Download;
        self.start_worker(ctx);
    }

    /// Поставить ссылку в очередь.
    fn enqueue(&mut self, url: String, opts: JobOpts, sub_url: Option<String>) {
        let id = self.next_id;
        self.next_id += 1;
        let title = match &sub_url {
            Some(_) => self.s.subscriptions.iter().find(|s| s.url == url).map(|s| s.name.clone()).filter(|n| !n.is_empty()),
            None => None,
        };
        self.jobs.push(Job {
            id,
            // Список треков из браузера: вместо «vgdlist:C:\…» — название плейлиста.
            title: title.or_else(|| vgd::music::list_title(&url)).unwrap_or_else(|| url.clone()),
            url,
            stage: "queued".into(),
            item: String::new(),
            percent: 0.0,
            speed: String::new(),
            eta: String::new(),
            message: String::new(),
            files: Vec::new(),
            state: State::Queued,
            error: String::new(),
            notice: String::new(),
            opts,
            sub_url,
        });
        self.sync_queue();
    }

    /// Очередь для рабочих нитей — ссылки «в очереди» в порядке карточек.
    fn sync_queue(&mut self) {
        let q: VecDeque<_> =
            self.jobs.iter().filter(|j| j.state == State::Queued).map(|j| (j.id, j.url.clone(), j.opts.clone())).collect();
        self.shared.lock().unwrap().queue = q;
        if self.status == "все загрузки завершены" {
            self.status.clear();
        }
    }

    /// Проверить подписки: новые видео/треки; при первой проверке — только запомнить, что уже есть.
    fn check_subs(&mut self, only: Option<usize>) {
        let subs = self.s.subscriptions.clone();
        for (i, sub) in subs.iter().enumerate() {
            if only.is_some_and(|o| o != i) {
                continue;
            }
            // Уже в очереди — не дублируем.
            if self.jobs.iter().any(|j| j.sub_url.as_deref() == Some(&sub.url) && matches!(j.state, State::Queued | State::Running | State::Paused)) {
                continue;
            }
            let opts = JobOpts { audio_only: sub.audio_only, max_age_days: sub.max_age_days, seed_only: !sub.seeded, ..Default::default() };
            self.enqueue(sub.url.clone(), opts, Some(sub.url.clone()));
        }
        let ctx = self.ctx.clone();
        self.start_worker(&ctx);
    }

    fn start_worker(&mut self, ctx: &egui::Context) {
        if !self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            self.shared.lock().unwrap().stop_all = false;
            let (shared, tx, ctx) = (self.shared.clone(), self.tx.clone(), ctx.clone());
            self.worker = Some(std::thread::spawn(move || work(shared, tx, ctx, false)));
        }
        // Небольшую музыку качаем рядом с основной нитью, не дожидаясь очереди.
        let side_busy = self.side_worker.as_ref().is_some_and(|w| !w.is_finished());
        let small = self.shared.lock().unwrap().queue.iter().any(|(_, u, _)| vgd::music::is_small_music(u));
        if !side_busy && small {
            let (shared, tx, ctx) = (self.shared.clone(), self.tx.clone(), ctx.clone());
            self.side_worker = Some(std::thread::spawn(move || work(shared, tx, ctx, true)));
        }
    }

    fn cancel_running(&self, id: u64) -> bool {
        let c = self.shared.lock().unwrap().current.iter().find(|(cid, _)| *cid == id).map(|(_, c)| c.clone());
        match c {
            Some(c) => {
                std::thread::spawn(move || c.cancel());
                true
            }
            None => false,
        }
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::Cancel(id) => {
                let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) else { return };
                match j.state {
                    State::Queued | State::Paused => {
                        j.state = State::Cancelled;
                        j.stage = "cancel".into();
                        j.message.clear();
                        self.sync_queue();
                    }
                    State::Running => {
                        self.pausing.remove(&id);
                        if self.cancel_running(id) {
                            self.status = "отменяю загрузку…".into();
                        }
                    }
                    _ => {}
                }
            }
            // Пауза: ссылка остаётся на своём месте, «продолжить» докачает с того же места
            // (скачанное пропустится по архиву, недокачанный файл yt-dlp продолжит).
            Action::Pause(id) => {
                let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) else { return };
                match j.state {
                    State::Queued => {
                        j.state = State::Paused;
                        j.stage = "paused".into();
                        self.sync_queue();
                    }
                    State::Running => {
                        j.message = "ставлю на паузу…".into();
                        self.pausing.insert(id);
                        self.cancel_running(id);
                    }
                    _ => {}
                }
            }
            Action::Resume(id) => {
                if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id && j.state == State::Paused) {
                    j.state = State::Queued;
                    j.stage = "queued".into();
                    j.message.clear();
                }
                self.sync_queue();
                let ctx = self.ctx.clone();
                self.start_worker(&ctx);
            }
            Action::Move(id, dir) => {
                if let Some(i) = self.jobs.iter().position(|j| j.id == id) {
                    let k = i as i64 + dir as i64;
                    if k >= 0 && (k as usize) < self.jobs.len() {
                        self.jobs.swap(i, k as usize);
                        self.sync_queue();
                    }
                }
            }
            Action::EditOpts(id) => {
                self.edit_job = if self.edit_job == Some(id) { None } else { Some(id) };
            }
            Action::Remove(id) => self.jobs.retain(|j| j.id != id),
            // Повторить отменённую или упавшую загрузку: скачанное раньше пропустится по архиву.
            Action::Retry(id) => {
                if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                    j.state = State::Queued;
                    j.stage = "queued".into();
                    j.error.clear();
                    j.notice.clear();
                    j.message.clear();
                    j.percent = 0.0;
                    j.files.clear();
                }
                self.sync_queue();
                let ctx = self.ctx.clone();
                self.start_worker(&ctx);
            }
            Action::Reveal(f) => reveal(Path::new(&f)),
        }
    }

    fn clear_done(&mut self) {
        self.jobs.retain(|j| matches!(j.state, State::Queued | State::Running | State::Paused));
    }

    fn update_ytdlp(&mut self, ctx: &egui::Context) {
        if self.busy() || self.side_task {
            info("Обновление", "Дождитесь окончания загрузок.");
            return;
        }
        self.side_task = true;
        self.status = "обновляю yt-dlp…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let log_tx = tx.clone();
            let r = update_ytdlp(&|m| {
                let _ = log_tx.send(Event::Log(m));
            });
            let _ = tx.send(match r {
                Ok(v) => Event::YtdlpVersion(v),
                Err(e) => Event::Status(format!("не удалось обновить yt-dlp: {e}")),
            });
            ctx.request_repaint();
        });
    }

    /// Заголовок группы настроек: щелчок сворачивает и разворачивает её (запоминается).
    fn group(&mut self, ui: &mut egui::Ui, title: &str) -> bool {
        let open = !self.s.collapsed.iter().any(|c| c == title);
        ui.add_space(12.0);
        let font = FontId::monospace(11.5);
        let size = text_size(ui, title, &font) + Vec2::new(18.0, 4.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if resp.hovered() { TEXT } else { MUTED };
        // треугольник: ▾ — открыта, ▸ — свёрнута
        let c = egui::pos2(rect.left() + 5.0, rect.center().y);
        let pts = if open {
            vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
        } else {
            vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.0, 4.0)]
        };
        ui.painter().add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        ui.painter().text(egui::pos2(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, title, font, color);
        if resp.clicked() {
            if open {
                self.s.collapsed.push(title.to_string());
            } else {
                self.s.collapsed.retain(|c| c != title);
            }
        }
        ui.add_space(4.0);
        open
    }

    /// Сменить папку загрузок (старая и новая остаются в списке недавних).
    fn set_out_dir(&mut self, dir: String) {
        let old = self.s.out_dir.clone();
        self.s.remember_dir(&old);
        self.s.out_dir = dir.clone();
        self.s.remember_dir(&dir);
        self.save_settings();
    }

    fn pick_out_dir(&mut self) {
        let start = if Path::new(&self.s.out_dir).is_dir() { PathBuf::from(&self.s.out_dir) } else { PathBuf::from(vgd::settings::default_out()) };
        if let Some(d) = rfd::FileDialog::new().set_directory(start).pick_folder() {
            self.set_out_dir(d.display().to_string());
        }
    }

    fn open_topics(&mut self) {
        self.topics_edit = Some(load_topics().into_iter().map(|(n, w)| (n.clone(), n, w.join(", "))).collect());
    }

    /// Сохранить категории из редактора; переименованные — переименовать и их папки в папке загрузок.
    fn save_topics_edit(&mut self, rows: Vec<(String, String, String)>) {
        let mut topics: Topics = Vec::new();
        for (orig, name, words) in &rows {
            let name = vgd::sort::safe_name(name.trim(), 80);
            if name.is_empty() || topics.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                continue;
            }
            let words: Vec<String> =
                words.split([',', '\n']).map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
            if !orig.is_empty() && *orig != name {
                let (from, to) = (Path::new(&self.s.out_dir).join(orig), Path::new(&self.s.out_dir).join(&name));
                if from.is_dir() && !to.exists() {
                    match std::fs::rename(&from, &to) {
                        Ok(_) => self.add_log(format!("Папка «{orig}» переименована в «{name}».")),
                        Err(e) => self.add_log(format!("Не удалось переименовать папку «{orig}»: {e}")),
                    }
                }
            }
            topics.push((name, words));
        }
        self.status = match save_topics(&topics) {
            Ok(_) => format!("категории сохранены: {}", topics.len()),
            Err(e) => e,
        };
    }

    fn sort_existing(&mut self, ctx: &egui::Context) {
        if self.busy() || self.side_task {
            info("Разложить", "Дождитесь окончания загрузок.");
            return;
        }
        self.save_settings();
        let root = PathBuf::from(&self.s.out_dir);
        if !root.is_dir() {
            return;
        }
        self.side_task = true;
        self.sort_root = root.clone();
        self.status = "ищу видео с [id] в имени и узнаю их каналы…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let log_tx = tx.clone();
            let r = plan_existing(
                &root,
                &|m| {
                    let _ = log_tx.send(Event::Log(m));
                },
                None,
            );
            let _ = tx.send(match r {
                Ok(plan) => Event::SortPlan(plan),
                Err(e) => Event::Status(format!("не удалось: {e}")),
            });
            ctx.request_repaint();
        });
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Progress(id, p) => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.state = State::Running;
                        if !p.title.is_empty() {
                            j.title = p.title.clone();
                        }
                        j.stage = p.stage.clone();
                        j.item = p.item.clone();
                        j.percent = p.percent;
                        j.speed = p.speed.clone();
                        j.eta = p.eta.clone();
                        j.message = p.message.clone();
                        if !p.notice.is_empty() {
                            j.notice = p.notice.clone();
                        }
                    }
                }
                Event::Log(m) => self.add_log(m),
                Event::Finish(id, state, files, err) => {
                    // Остановили ради паузы — карточка «на паузе», а не «отменено».
                    let state = if self.pausing.remove(&id) && state == State::Cancelled { State::Paused } else { state };
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.state = state;
                        j.stage = match state {
                            State::Done => "done",
                            State::Error => "error",
                            State::Paused => "paused",
                            _ => "cancel",
                        }
                        .into();
                        j.percent = if state == State::Done { 100.0 } else { 0.0 };
                        j.speed.clear();
                        j.eta.clear();
                        j.item.clear();
                        j.message = if state == State::Done {
                            if files.is_empty() {
                                "новых видео нет — всё уже скачано раньше".into()
                            } else {
                                format!("файлов: {}", files.len())
                            }
                        } else {
                            String::new()
                        };
                        j.files = files;
                        j.error = err
                            .map(|e| e.lines().next().unwrap_or("").chars().take(200).collect())
                            .unwrap_or_default();
                    }
                    let job = self.jobs.iter().find(|j| j.id == id).map(|j| (j.files.clone(), j.title.clone(), j.sub_url.clone(), j.state));
                    if let Some((files, title, sub_url, st)) = job {
                        // Подписка проверена: запоминаем, что «старое» уже учтено, и время проверки.
                        if let (Some(url), State::Done) = (sub_url, st) {
                            if let Some(sub) = self.s.subscriptions.iter_mut().find(|s| s.url == url) {
                                sub.seeded = true;
                                sub.last_check = now_secs();
                                if sub.name.is_empty() && !title.starts_with("http") {
                                    sub.name = title.clone();
                                }
                            }
                            self.save_settings();
                        }
                        // Уведомление Windows: щелчок открывает папку с файлом.
                        if self.s.notify_done && st == State::Done && !files.is_empty() {
                            let dir = Path::new(&files[0]).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(&self.s.out_dir));
                            vgd::appinfo::toast("Скачано", &format!("{title} · файлов: {}", files.len()), &dir);
                        }
                    }
                }
                Event::Idle => {
                    let active = self.jobs.iter().any(|j| matches!(j.state, State::Queued | State::Running));
                    if !active && !self.jobs.iter().any(|j| j.state == State::Error) && !self.jobs.is_empty() {
                        self.status = "все загрузки завершены".into();
                    }
                    if !active && self.status.starts_with("отменяю") {
                        self.status.clear();
                    }
                    // Нить закончила, а в очереди что-то появилось, пока она останавливалась, — запустить снова.
                    if self.jobs.iter().any(|j| j.state == State::Queued) {
                        self.start_worker(ctx);
                    }
                }
                Event::YandexCode(c) => {
                    if c.is_none() {
                        self.side_task = false;
                    }
                    self.ya_code = c;
                }
                Event::SpotifyDone(r) => {
                    self.side_task = false;
                    match r {
                        Ok(()) => {
                            self.s.spotify_app = true;
                            self.save_settings();
                            if let Ok(exe) = std::env::current_exe() {
                                vgd::spotify_app::set_autorepair(true, &exe);
                            }
                            self.status = "кнопки «скачать» в spotify готовы".into();
                        }
                        Err(e) => {
                            self.add_log(format!("Ошибка: Spotify: {e}"));
                            self.status = format!("spotify: {e}");
                        }
                    }
                }
                Event::Status(s) => {
                    self.side_task = false;
                    self.status = s;
                }
                Event::YtdlpVersion(v) => {
                    if self.side_task {
                        self.status = format!("yt-dlp: {v}");
                    }
                    self.side_task = false;
                    self.ytdlp = v;
                }
                Event::External(urls) => {
                    // Показать окно поверх остальных и сразу начать загрузку.
                    self.show_window(ctx);
                    if !urls.is_empty() {
                        let n = urls.len();
                        self.urls = urls.join("
");
                        self.add_and_start(ctx);
                        self.add_log(format!("Из браузера: ссылок {n}"));
                    }
                }
                Event::SortPlan(plan) => {
                    self.side_task = false;
                    if plan.is_empty() {
                        self.status = "раскладывать нечего".into();
                        info("Разложить", "В корне папки нет неразложенных видео с [id] в имени.");
                    } else {
                        self.status = format!("найдено файлов для раскладки: {}", plan.len());
                        self.sort_plan = Some(plan);
                    }
                }
            }
        }
    }

    // ---------- боковая панель ----------

    fn ui_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(28.0)));
            }
            ui.add_space(14.0);
            let running = self.jobs.iter().filter(|j| matches!(j.state, State::Queued | State::Running)).count();
            for (page, icon, label) in [
                (Page::Download, I_DOWNLOAD, "скачать"),
                (Page::Subs, I_SUBS, "подписки"),
                (Page::Settings, I_SETTINGS, "настройки"),
                (Page::Log, I_LOG, "журнал"),
            ] {
                let badge = if page == Page::Download && running > 0 { Some(running) } else { None };
                if nav_item(ui, icon, label, self.page == page, badge).clicked() {
                    self.page = page;
                }
                ui.add_space(2.0);
            }
        });
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("v{APP_VERSION}")).size(10.5).color(FAINT));
        });
    }

    // ---------- скачать ----------

    fn ui_download(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().id_salt("download").auto_shrink([false, false]).show(ui, |ui| {
            centered(ui, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("very good downloader").size(17.0).color(TEXT));
                    ui.label(RichText::new("youtube · spotify · apple music · soundcloud · яндекс музыка · vk…").size(11.5).color(FAINT));
                });
                ui.add_space(10.0);

                // поле для ссылок
                let focused_id = egui::Id::unique("urls");
                let has_focus = ui.memory(|m| m.has_focus(focused_id));
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, if has_focus { MUTED } else { BORDER }))
                    .corner_radius(12)
                    .inner_margin(Margin { left: 12, right: 12, top: 9, bottom: 9 })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(I_LINK.to_string()).family(icons()).size(15.0).color(MUTED));
                            ui.add(
                                egui::TextEdit::multiline(&mut self.urls)
                                    .id(focused_id)
                                    .frame(egui::Frame::NONE)
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(
                                        RichText::new("вставь ссылку — несколько можно с новой строки").color(FAINT),
                                    ),
                            );
                        });
                    });
                ui.add_space(8.0);

                // Музыкальная ссылка в поле — вместо качества видео показываем формат звука.
                let music_mode = self.urls.lines().any(|l| vgd::music::is_music_url(l.trim()));
                let spotify = self.urls.to_lowercase().contains("spotify");
                ui.horizontal(|ui| {
                    if music_mode {
                        let mut f = self.s.music_format.clone();
                        let opts = vec![
                            ("best".to_string(), "как есть".to_string()),
                            ("mp3".to_string(), "mp3 320".to_string()),
                            ("m4a".to_string(), "m4a".to_string()),
                            ("flac".to_string(), "flac".to_string()),
                        ];
                        if segmented(ui, &mut f, &opts) {
                            self.s.music_format = f;
                            self.save_settings();
                        }
                    } else {
                        let mut h = self.s.max_height;
                        let opts: Vec<(u32, String)> =
                            [0, 2160, 1440, 1080, 720].iter().map(|v| (*v, height_short(*v).to_string())).collect();
                        if segmented(ui, &mut h, &opts) {
                            self.s.max_height = h;
                            self.save_settings();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pill(ui, Some(I_DOWNLOAD), "скачать", true, true).on_hover_text("Ctrl+Enter").clicked() {
                            self.add_and_start(&ctx);
                        }
                        if pill(ui, Some(I_PASTE), "вставить", false, true).on_hover_text("вставить ссылку из буфера и сразу скачать").clicked() {
                            self.paste();
                            self.add_and_start(&ctx);
                        }
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("открыть папку загрузок");
                        ui.painter().rect_filled(r, 11, if resp.hovered() { SURFACE_HI } else { SURFACE });
                        ui.painter().rect_stroke(r, 11, Stroke::new(1.0, BORDER), StrokeKind::Inside);
                        paint_folder(ui.painter(), r.center(), TEXT);
                        if resp.clicked() {
                            let _ = std::fs::create_dir_all(&self.s.out_dir);
                            open_path(Path::new(&self.s.out_dir));
                        }
                    });
                });
                ui.add_space(6.0);
                // кратко о текущих настройках — щелчок открывает настройки
                if spotify {
                    egui::Frame::new().fill(Color32::from_rgb(0x1d, 0x18, 0x0c)).corner_radius(10).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(
                            "spotify отдаёт программе только первые 100 треков плейлиста. если треков больше — откройте плейлист                              в браузере и нажмите там кнопку «скачать плейлист» (расширение very good downloader): она передаст весь список.",
                        ).size(11.5).color(WARN)).wrap());
                    });
                    ui.add_space(6.0);
                }
                if self.s.ext_version.is_empty() && ipc::extension_version().is_none() {
                    if link(ui, "↓ кнопки скачивания прямо на сайтах — установить расширение для браузера").clicked() {
                        self.page = Page::Settings;
                    }
                    ui.add_space(4.0);
                }
                // Скопированная ссылка — предложить скачать.
                if let Some(url) = self.clip_offer.clone() {
                    egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, BORDER)).corner_radius(10).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("скопирована ссылка:").size(12.0).color(MUTED));
                            let w = ui.available_width() - 150.0;
                            ui.allocate_ui(Vec2::new(w.max(60.0), 20.0), |ui| {
                                ui.add(egui::Label::new(RichText::new(&url).size(12.0).color(TEXT)).truncate());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, I_CLOSE, "не надо").clicked() {
                                    self.clip_offer = None;
                                }
                                if small_pill(ui, "скачать").clicked() {
                                    self.clip_offer = None;
                                    self.urls = url.clone();
                                    self.add_and_start(&ctx);
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
                // Параметры для следующих ссылок: фрагмент «с / до» и «только звук».
                ui.horizontal(|ui| {
                    let custom = self.next_opts.is_variant() || !self.next_opts.preset.is_empty() || !self.next_opts.over.is_empty();
                    let label = if custom {
                        let mut parts = Vec::new();
                        if !self.next_opts.preset.is_empty() {
                            parts.push(format!("пресет «{}»", self.next_opts.preset));
                        }
                        if !self.next_opts.over.is_empty() {
                            parts.push("свои настройки".into());
                        }
                        if self.next_opts.has_section() {
                            parts.push(format!("фрагмент {}–{}", self.next_opts.from.trim().if_empty("начало"), self.next_opts.to.trim().if_empty("конец")));
                        }
                        if self.next_opts.audio_only {
                            parts.push("только звук".into());
                        }
                        if self.next_opts.split_tracks {
                            parts.push("микс → треки".into());
                        }
                        format!("параметры: {}", parts.join(" · "))
                    } else {
                        "параметры ▾".to_string()
                    };
                    if link(ui, &label).clicked() {
                        self.show_opts = !self.show_opts;
                    }
                    if custom && link(ui, "сбросить").clicked() {
                        self.next_opts = JobOpts::default();
                    }
                });
                if self.show_opts {
                    ui.add_space(4.0);
                    card(ui, |ui| {
                        row(ui, "фрагмент", "скачать только часть видео: время «с» и «до» — 1:30, 01:02:03 или секунды. пустое поле — с начала / до конца. края режутся точно по кадру.", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.next_opts.to).hint_text("до").desired_width(70.0));
                            ui.label(RichText::new("—").color(MUTED));
                            ui.add(egui::TextEdit::singleline(&mut self.next_opts.from).hint_text("с").desired_width(70.0));
                        });
                        divider(ui);
                        row(ui, "только звук", "для видео — сохранить только звук (формат как в «музыка»).", |ui| {
                            toggle(ui, &mut self.next_opts.audio_only);
                        });
                        divider(ui);
                        row(ui, "микс → отдельные треки", "микс или альбом одним видео режется на треки: по главам youtube, а если их нет — по таймкодам в описании или в закреплённом комментарии. треки — в «музыка\\<название видео>», формат как в настройках музыки, теги: альбом — название видео, номер, название трека, обложка.", |ui| {
                            toggle(ui, &mut self.next_opts.split_tracks);
                        });
                    });
                    ui.add_space(4.0);
                    opts_editor(ui, &self.s, &mut self.next_opts, music_mode, true);
                    ui.add_space(4.0);
                }
                // Папка загрузок (щелчок — недавние папки и «другая папка…») и кратко о настройках (щелчок — настройки).
                ui.horizontal(|ui| {
                    if link(ui, &format!("папка: {} ▾", short_dir(&self.s.out_dir))).on_hover_text(&self.s.out_dir).clicked() {
                        self.show_folders = !self.show_folders;
                    }
                    ui.label(RichText::new("·").size(11.5).color(FAINT));
                    let summary = if music_mode { self.music_summary() } else { self.summary() };
                    let r = ui.add(egui::Label::new(RichText::new(summary).size(11.5).color(FAINT)).sense(Sense::click()).truncate());
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("открыть настройки").clicked() {
                        self.page = Page::Settings;
                    }
                });
                if self.show_folders {
                    ui.add_space(4.0);
                    let mut pick: Option<String> = None;
                    let mut other = false;
                    let current = self.s.out_dir.clone();
                    let recent: Vec<String> = self
                        .s
                        .recent_dirs
                        .iter()
                        .filter(|d| !d.eq_ignore_ascii_case(&current) && Path::new(d.as_str()).is_dir())
                        .cloned()
                        .collect();
                    card(ui, |ui| {
                        for d in std::iter::once(&current).chain(recent.iter()) {
                            let is_current = *d == current;
                            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 32.0), Layout::left_to_right(Align::Center), |ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                                paint_folder(ui.painter(), r.center(), if is_current { TEXT } else { MUTED });
                                let w = (ui.available_width() - 90.0).max(60.0);
                                ui.allocate_ui(Vec2::new(w, 20.0), |ui| {
                                    ui.add(egui::Label::new(RichText::new(d.as_str()).size(12.5).color(if is_current { TEXT } else { MUTED })).truncate());
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if is_current {
                                        ui.label(RichText::new("сейчас").size(11.5).color(OK));
                                    } else if small_pill(ui, "выбрать").clicked() {
                                        pick = Some(d.clone());
                                    }
                                });
                            });
                        }
                        divider(ui);
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
                            if small_pill(ui, "другая папка…").clicked() {
                                other = true;
                            }
                            if small_pill(ui, "открыть").clicked() {
                                let _ = std::fs::create_dir_all(&current);
                                open_path(Path::new(&current));
                            }
                        });
                    });
                    if let Some(d) = pick {
                        self.set_out_dir(d);
                        self.show_folders = false;
                    }
                    if other {
                        self.pick_out_dir();
                        self.show_folders = false;
                    }
                }

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("очередь").size(14.0).color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.jobs.iter().any(|j| !matches!(j.state, State::Queued | State::Running))
                            && link(ui, "убрать готовые").clicked()
                        {
                            self.clear_done();
                        }
                    });
                });
                ui.add_space(4.0);
                if self.jobs.is_empty() {
                    egui::Frame::new().stroke(Stroke::new(1.0, Color32::from_rgb(0x1c, 0x1c, 0x1c))).corner_radius(12).inner_margin(14).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("пусто — вставь ссылку выше или нажми «скачать» в браузере").size(12.0).color(FAINT));
                    });
                }
                let mut actions = Vec::new();
                let n = self.jobs.len();
                let mut opts_changed = false;
                for i in 0..n {
                    job_card(ui, &self.jobs[i], i == 0, i + 1 == n, &mut actions);
                    let j = &mut self.jobs[i];
                    if self.edit_job == Some(j.id) && matches!(j.state, State::Queued | State::Paused) {
                        ui.add_space(2.0);
                        let music = vgd::music::is_music_url(&j.url);
                        if opts_editor(ui, &self.s, &mut j.opts, music, false) {
                            opts_changed = true;
                        }
                    }
                    ui.add_space(6.0);
                }
                if opts_changed {
                    self.sync_queue();
                }
                for a in actions {
                    self.apply(a);
                }
                if !self.status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.status).size(11.5).color(MUTED));
                }
                ui.add_space(16.0);
            });
        });
    }

    fn music_summary(&self) -> String {
        let fmt = match self.s.music_format.as_str() {
            "mp3" => "mp3 320",
            "m4a" => "m4a",
            "flac" => "flac",
            _ => "лучший звук без перекодирования",
        };
        format!(r"музыка  ·  {fmt}  ·  теги и обложки  ·  …\Музыка")
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(match self.s.edit.as_str() {
            "h264" => "копия h.264".to_string(),
            "prores" => "копия prores".to_string(),
            _ => "без копии для монтажа".to_string(),
        });
        if self.s.max_fps > 0 {
            parts.push(format!("до {} fps", self.s.max_fps));
        }
        if self.s.sort {
            parts.push("категория / канал".into());
        }
        parts.join("  ·  ")
    }

    // ---------- настройки ----------

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let before = self.s.clone();
        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            centered(ui, |ui| {
                ui.add_space(18.0);
                ui.label(RichText::new("настройки").size(17.0).color(TEXT));
                ui.add_space(10.0);

                if self.group(ui, "расширение для браузера") || self.ext_wizard {
                    self.ui_extension(ui);
                }

                if self.group(ui, "приложение spotify") {
                    self.ui_spotify_app(ui);
                }

                if self.group(ui, "сохранение") {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                        paint_folder(ui.painter(), r.center(), MUTED);
                        let w = (ui.available_width() - 190.0).max(80.0);
                        ui.add(egui::TextEdit::singleline(&mut self.s.out_dir).frame(egui::Frame::NONE).desired_width(w));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_pill(ui, "открыть").clicked() {
                                let _ = std::fs::create_dir_all(&self.s.out_dir);
                                open_path(Path::new(&self.s.out_dir));
                            }
                            if small_pill(ui, "выбрать").clicked() {
                                let start = if Path::new(&self.s.out_dir).is_dir() {
                                    PathBuf::from(&self.s.out_dir)
                                } else {
                                    PathBuf::from(vgd::settings::default_out())
                                };
                                if let Some(d) = rfd::FileDialog::new().set_directory(start).pick_folder() {
                                    self.set_out_dir(d.display().to_string());
                                }
                            }
                        });
                    });
                    divider(ui);
                    row(ui, "раскладывать по папкам категория / канал", "категория — по ключевым словам в названии, тегах, нике и плейлисте, иначе по рубрике youtube или названию сайта. похожие ники попадают в одну папку. категории и их слова настраиваются кнопкой «категории…» ниже.", |ui| {
                        toggle(ui, &mut self.s.sort);
                    });
                    divider(ui);
                    row(ui, "плейлисты и каналы целиком", "выключено — из ссылки с плейлистом берётся только само видео.", |ui| {
                        toggle(ui, &mut self.s.playlist);
                    });
                    divider(ui);
                    row(ui, "категории и уже скачанное", "«категории» — папки, по которым раскладываются видео, и слова, по которым они выбираются. «разложить» — разложить уже скачанные файлы с [id] в имени: сначала покажет план.", |ui| {
                        if small_pill_enabled(ui, "разложить…", !self.side_task).clicked() {
                            self.sort_existing(&ctx);
                        }
                        if small_pill(ui, "категории…").clicked() {
                            self.open_topics();
                        }
                    });
                });
                }

                if self.group(ui, "видео") {
                card(ui, |ui| {
                    row(ui, "качество", "по меньшей стороне кадра: вертикальное 1080×1920 — это 1080p. если такого нет, берётся ближайшее ниже.", |ui| {
                        let opts: Vec<(u32, String)> =
                            vgd::settings::HEIGHTS.iter().map(|h| (*h, height_short(*h).to_string())).collect();
                        segmented(ui, &mut self.s.max_height, &opts);
                    });
                    divider(ui);
                    row(ui, "макс. fps", "если на сайте нет формата с нужной частотой, видео пересчитывается в h.264 mp4.", |ui| {
                        let opts: Vec<(u32, String)> = vgd::settings::FPS_LIMITS
                            .iter()
                            .map(|f| (*f, if *f == 0 { "нет".to_string() } else { f.to_string() }))
                            .collect();
                        segmented(ui, &mut self.s.max_fps, &opts);
                    });
                    divider(ui);
                    row(ui, "копия для монтажа", "оригинал сохраняется без перекодирования (mkv). after effects и premiere не открывают vp9/av1/opus — рядом кладётся копия h.264 mp4 или prores mov. hdr переводится в sdr.", |ui| {
                        let opts = vec![
                            ("none".to_string(), "нет".to_string()),
                            ("h264".to_string(), "h.264".to_string()),
                            ("prores".to_string(), "prores".to_string()),
                        ];
                        segmented(ui, &mut self.s.edit, &opts);
                    });
                    if self.s.edit != "none" {
                        divider(ui);
                        row(ui, "вместо оригинала", "видео сразу сохраняется в выбранном формате (h.264 mp4 или prores mov), оригинал mkv не остаётся. выключено — оригинал + копия «…_edit» рядом.", |ui| {
                            toggle(ui, &mut self.s.edit_replace);
                        });
                    }
                    divider(ui);
                    row(ui, "обложка .jpg", "картинка-обложка рядом с видео.", |ui| {
                        toggle(ui, &mut self.s.thumbnail);
                    });
                    divider(ui);
                    row(ui, "вырезать рекламу (sponsorblock)", "по базе sponsorblock вырезаются спонсорские вставки, самореклама и просьбы подписаться. работает для youtube.", |ui| {
                        toggle(ui, &mut self.s.sponsorblock);
                    });
                    divider(ui);
                    row(ui, "разбить по главам", "если у видео есть главы, рядом появится папка «… — главы» с отдельным файлом на каждую.", |ui| {
                        toggle(ui, &mut self.s.split_chapters);
                    });
                    divider(ui);
                    row(ui, "файл «откуда скачано»", "в папке со скачанным появляется «_откуда скачано.txt»: дата, имя файла, сайт и ссылка, откуда он взят. для музыки — и пометка «замена», если трек нашёлся не там, где его просили.", |ui| {
                        toggle(ui, &mut self.s.source_log);
                    });
                    divider(ui);
                    row(ui, "субтитры", "языки через запятую. автосубтитры берутся только на языке оригинала: автоперевод youtube почти всегда отвечает ошибкой 429.", |ui| {
                        toggle(ui, &mut self.s.subs);
                        if self.s.subs {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.s.sub_langs)
                                    .desired_width(70.0)
                                    .align(Align2::CENTER_CENTER),
                            );
                        }
                    });
                    if self.s.subs {
                        divider(ui);
                        row(ui, "автосубтитры (язык оригинала)", "", |ui| {
                            toggle(ui, &mut self.s.auto_subs);
                        });
                    }
                });
                }

                if self.group(ui, "музыка") {
                card(ui, |ui| {
                    row(ui, "формат", "«как есть» — лучший звук сервиса без перекодирования (opus или m4a). mp3 320 и m4a — для старых плееров и машин. flac не добавит качества к сжатому источнику, но нужен некоторым программам. hi-res (24 бит) сервисы бесплатно не отдают.", |ui| {
                        let opts = vec![
                            ("best".to_string(), "как есть".to_string()),
                            ("mp3".to_string(), "mp3 320".to_string()),
                            ("m4a".to_string(), "m4a".to_string()),
                            ("flac".to_string(), "flac".to_string()),
                        ];
                        segmented(ui, &mut self.s.music_format, &opts);
                    });
                    divider(ui);
                    row(ui, "номер трека в имени файла", "«01 - исполнитель - название» — файлы идут по порядку плейлиста в любой папке. выключено — «исполнитель - название», порядок остаётся в плейлисте .m3u8 и тегах.", |ui| {
                        toggle(ui, &mut self.s.music_numbers);
                    });
                    divider(ui);
                    row(ui, "тексты песен", "текст с lrclib.net: внутри файла (видно в плеерах) и .lrc рядом — для бегущего текста.", |ui| {
                        toggle(ui, &mut self.s.lyrics);
                    });
                    divider(ui);
                    row(ui, "искать замену недоступным трекам", "трек есть в vk, soundcloud или youtube music, но не скачивается (заблокирован, удалён, нет прав) — программа ищет его на youtube music, soundcloud, audius и youtube. выключено — такой трек пропускается и попадает в журнал. для spotify, apple и яндекса поиск работает всегда: их звук скачать нельзя.", |ui| {
                        let mut on = !self.s.music_no_replace;
                        if toggle(ui, &mut on) {
                            self.s.music_no_replace = !on;
                        }
                    });
                    if !self.s.music_no_replace {
                        divider(ui);
                        row(ui, "замены — в отдельную папку", "треки, найденные не там, где их просили, кладутся в подпапку «_из других мест» внутри плейлиста.", |ui| {
                            toggle(ui, &mut self.s.music_replace_dir);
                        });
                    }
                    divider(ui);
                    row(ui, "сверять замену по звуку", "найденный на youtube, soundcloud или audius трек сравнивается по звуку с 30-секундным отрывком оригинала с deezer. не совпал (кавер, ремикс, sped up, другая песня с тем же названием) — программа ищет дальше. если отрывка на deezer нет, проверка пропускается.", |ui| {
                        toggle(ui, &mut self.s.music_verify);
                    });
                    divider(ui);
                    row(ui, "искать в vk (ваш аккаунт)", "замена треку ищется сначала в vk — от вашего аккаунта, как в самом vk: часто mp3 320 и редкие треки. программа открывает vk.com в невидимом окне (движок edge webview2); войти нужно один раз кнопкой «войти в vk» — пароль вводится на странице vk, программа его не видит. найденное сверяется по звуку, как и с других сайтов.", |ui| {
                        let logged = vgd::vk_web::logged_in();
                        toggle(ui, &mut self.s.vk_search);
                        if small_pill(ui, if logged { "войти заново" } else { "войти в vk" }).clicked() {
                            if let Err(e) = vgd::vk_web::open_login() {
                                self.status = format!("vk: {e}");
                            }
                        }
                        ui.label(RichText::new(if logged { "вход выполнен" } else { "нет входа" }).size(11.5).color(if logged { OK } else { MUTED }));
                    });
                    divider(ui);
                    row(ui, "яндекс музыка (ваш аккаунт)", "вход кодом: программа покажет код и откроет страницу яндекса — подтвердите вход там (пароль программа не видит). с подпиской плюс треки яндекса качаются прямо с яндекса (flac, если есть, иначе mp3 320 / aac), а для spotify, apple и других яндекс ищется сразу после vk. без подписки яндекс отдаёт только отрывки — тогда трек ищется на других сайтах. зашифрованные файлы яндекса программа не берёт.", |ui| {
                        let logged = vgd::yandex_acc::logged_in();
                        toggle(ui, &mut self.s.yandex_acc);
                        if logged {
                            if small_pill(ui, "выйти").clicked() {
                                vgd::yandex_acc::logout();
                            }
                        } else if small_pill_enabled(ui, "войти в яндекс", !self.side_task).clicked() && !self.side_task {
                            self.side_task = true;
                            let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                            std::thread::spawn(move || {
                                let stx = tx.clone();
                                let sctx = ctx.clone();
                                let r = vgd::yandex_acc::login(&|code, url| {
                                    let _ = stx.send(Event::YandexCode(Some((code.to_string(), url.to_string()))));
                                    let _ = stx.send(Event::Log(format!("Вход в Яндекс Музыку: код {code} — {url}")));
                                    vgd::appinfo::open_url(url);
                                    sctx.request_repaint();
                                });
                                let _ = tx.send(Event::YandexCode(None));
                                let _ = tx.send(Event::Status(match r {
                                    Ok(()) => "яндекс музыка: вход выполнен".into(),
                                    Err(e) => format!("яндекс музыка: {e}"),
                                }));
                                ctx.request_repaint();
                            });
                        }
                        ui.label(RichText::new(if logged { "вход выполнен" } else { "нет входа" }).size(11.5).color(if logged { OK } else { MUTED }));
                    });
                    // Идёт вход: код крупно — его вводят на странице Яндекса.
                    if let Some((code, url)) = self.ya_code.clone() {
                        egui::Frame::new().fill(SURFACE_HI).corner_radius(10).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("введите этот код на странице яндекса — латиницей (английская раскладка):").size(12.0).color(MUTED));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&code).size(26.0).color(TEXT).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_pill(ui, "открыть страницу").clicked() {
                                        vgd::appinfo::open_url(&url);
                                    }
                                    if small_pill(ui, "копировать").clicked() {
                                        if let Ok(mut c) = arboard::Clipboard::new() {
                                            let _ = c.set_text(code.clone());
                                        }
                                    }
                                });
                            });
                            ui.label(RichText::new("после подтверждения программа заметит вход сама.").size(11.0).color(FAINT));
                        });
                    }
                    divider(ui);
                    row(ui, "ключ song.link (необязательно)", "song.link (odesli) по треку spotify, apple, яндекса или deezer даёт точные ссылки на тот же трек на youtube music, soundcloud и youtube — без поиска по названию. с осени 2026 их api работает только с ключом (запрос — на odesli.co). без ключа программа просто ищет как обычно.", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.s.odesli_key).password(true).hint_text("ключ").desired_width(180.0));
                    });
                    divider(ui);
                    row(ui, "ключ spotify (необязательно)", "проще всего большие плейлисты качать кнопкой расширения «скачать плейлист» на странице spotify — ключ не нужен. ключ — запасной путь: developer.spotify.com → dashboard → create app; в поле redirect uri впишите https://example.com/callback (адрес не используется); галочка web api → save → settings → скопируйте client id и client secret сюда.", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.s.spotify_secret).password(true).hint_text("client secret").desired_width(150.0));
                        ui.add(egui::TextEdit::singleline(&mut self.s.spotify_id).hint_text("client id").desired_width(150.0));
                    });
                    divider(ui);
                    row(ui, "spotify и vk из браузера", "spotify отдаёт программе только 100 треков, а vk — ничего без входа. откройте плейлист в браузере и нажмите кнопку расширения «скачать плейлист» — весь список уйдёт в программу. звук vk берётся прямо из vk; звук spotify — с youtube music, soundcloud или youtube. так же заменяются заблокированные треки.", |_ui| {});
                });
                }

                if self.group(ui, "пресеты") {
                card(ui, |ui| {
                    let mut remove = None;
                    let mut update = None;
                    for (i, p) in self.s.presets.iter().enumerate() {
                        row(ui, &p.name.to_lowercase(), &preset_summary(&p.values), |ui| {
                            if icon_button(ui, I_CLOSE, "удалить пресет").clicked() {
                                remove = Some(i);
                            }
                            if small_pill(ui, "взять текущие").on_hover_text("записать в пресет текущие настройки программы").clicked() {
                                update = Some(i);
                            }
                        });
                        divider(ui);
                    }
                    if let Some(i) = update {
                        self.s.presets[i].values = self.s.preset_values();
                    }
                    if let Some(i) = remove {
                        self.s.presets.remove(i);
                    }
                    row(ui, "сохранить текущие как пресет", "пресет запоминает папку, качество, fps, копию для монтажа, формат музыки, субтитры, обложку, sponsorblock, главы, раскладку по папкам и тексты песен. выбрать его можно в «параметры ▾» на главной или кнопкой ⚙ у ссылки в очереди.", |ui| {
                        let name = self.preset_new.trim().to_string();
                        let ok = !name.is_empty();
                        if small_pill_enabled(ui, "сохранить", ok).clicked() && ok {
                            let values = self.s.preset_values();
                            match self.s.presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&name)) {
                                Some(p) => p.values = values,
                                None => self.s.presets.push(vgd::settings::Preset { name, values }),
                            }
                            self.preset_new.clear();
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.preset_new).hint_text("название").desired_width(150.0));
                    });
                });
                }

                if self.group(ui, "интерфейс") {
                card(ui, |ui| {
                    row(ui, "предлагать скачать скопированную ссылку", "когда в буфере обмена появляется ссылка на видео или музыку, на главной появляется плашка «скачать».", |ui| {
                        toggle(ui, &mut self.s.clipboard_watch);
                    });
                    divider(ui);
                    row(ui, "уведомление, когда скачалось", "уведомление windows; щелчок по нему открывает папку с файлом.", |ui| {
                        toggle(ui, &mut self.s.notify_done);
                    });
                    divider(ui);
                    row(ui, "закрывать в трей", "крестик прячет окно в значок у часов — программа и загрузки продолжают работать, кнопки в браузере и spotify отправляют ссылки сразу. щелчок по значку — открыть окно, правая кнопка — «выход». кнопка «свернуть» сворачивает на панель задач как обычно.", |ui| {
                        toggle(ui, &mut self.s.tray);
                    });
                    divider(ui);
                    row(ui, "масштаб", "размер всего интерфейса. запоминается вместе с размером окна.", |ui| {
                        let opts: Vec<(u32, String)> =
                            [80u32, 90, 100, 110, 125, 150].iter().map(|p| (*p, format!("{p}%"))).collect();
                        let mut cur = (self.s.ui_scale * 100.0).round() as u32;
                        if segmented(ui, &mut cur, &opts) {
                            self.s.ui_scale = cur as f32 / 100.0;
                            self.pending_zoom = Some(self.s.ui_scale);
                        }
                    });
                });
                }

                if self.group(ui, "доступ и обновления") {
                card(ui, |ui| {
                    row(ui, "куки для 18+ и закрытых", "берутся, только если без входа видео недоступно: с ними youtube отдаёт меньше форматов. chrome и edge шифруют куки — лучше firefox или файл cookies.txt.", |ui| {
                        let prev = self.s.cookies.clone();
                        egui::ComboBox::from_id_salt("cookies")
                            .width(120.0)
                            .selected_text(browser_name(&self.s.cookies))
                            .show_ui(ui, |ui| {
                                for b in BROWSERS {
                                    ui.selectable_value(&mut self.s.cookies, b.to_string(), browser_name(b));
                                }
                            });
                        if self.s.cookies == "file" && prev != "file" {
                            match rfd::FileDialog::new()
                                .set_title("Файл cookies.txt (формат Netscape)")
                                .add_filter("cookies.txt", &["txt"])
                                .pick_file()
                            {
                                Some(f) => self.s.cookies_file = f.display().to_string(),
                                None => self.s.cookies = prev,
                            }
                        }
                    });
                    divider(ui);
                    let ver = format!("yt-dlp {}", self.ytdlp);
                    row(ui, &ver, "youtube часто меняется — если видео перестали скачиваться, обновите yt-dlp.", |ui| {
                        if small_pill_enabled(ui, "обновить", !self.side_task && !self.busy()).clicked() {
                            self.update_ytdlp(&ctx);
                        }
                    });
                });
                }
                if !self.status.is_empty() && self.side_task {
                    hint(ui, &self.status);
                }
                ui.add_space(14.0);
                ui.label(RichText::new(format!("{APP_NAME} {APP_VERSION} · yt-dlp + ffmpeg · настройки сохраняются сразу")).size(10.5).color(FAINT));
                ui.add_space(14.0);
            });
        });
        if !same(&before, &self.s) {
            self.save_settings();
        }
    }

    // ---------- подписки ----------

    fn ui_subs(&mut self, ui: &mut egui::Ui) {
        let before = self.s.clone();
        egui::ScrollArea::vertical().id_salt("subs").auto_shrink([false, false]).show(ui, |ui| {
            centered(ui, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("подписки").size(17.0).color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_pill_enabled(ui, "проверить все", !self.s.subscriptions.is_empty()).clicked() {
                            self.check_subs(None);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(
                    "каналы и плейлисты (youtube, soundcloud, youtube music, tiktok…): программа качает только новые видео и треки. \
                     при первой проверке она запоминает, что уже есть, — старое не качается.",
                ).size(11.5).color(MUTED)).wrap());
                ui.add_space(10.0);
                // добавить
                egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, BORDER)).corner_radius(12).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(I_LINK.to_string()).family(icons()).size(15.0).color(MUTED));
                        let w = ui.available_width() - 110.0;
                        ui.add(egui::TextEdit::singleline(&mut self.sub_new).frame(egui::Frame::NONE).desired_width(w).hint_text(RichText::new("ссылка на канал или плейлист").color(FAINT)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let url = self.sub_new.trim().to_string();
                            if small_pill_enabled(ui, "добавить", ipc::is_web_url(&url)).clicked() {
                                if !self.s.subscriptions.iter().any(|x| x.url == url) {
                                    self.s.subscriptions.push(Subscription { url: url.clone(), ..Default::default() });
                                    let i = self.s.subscriptions.len() - 1;
                                    self.save_settings();
                                    self.check_subs(Some(i));
                                }
                                self.sub_new.clear();
                            }
                        });
                    });
                });
                ui.add_space(10.0);
                if self.s.subscriptions.is_empty() {
                    ui.label(RichText::new("пока нет подписок").size(12.0).color(FAINT));
                }
                let mut remove = None;
                let mut check = None;
                for (i, sub) in self.s.subscriptions.iter_mut().enumerate() {
                    egui::Frame::new().fill(SURFACE).corner_radius(12).inner_margin(Margin { left: 14, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let title = if sub.name.is_empty() { sub.url.clone() } else { sub.name.clone() };
                            let w = ui.available_width() - 70.0;
                            ui.allocate_ui(Vec2::new(w.max(60.0), 20.0), |ui| {
                                ui.add(egui::Label::new(RichText::new(title).size(13.5).color(TEXT)).truncate()).on_hover_text(&sub.url);
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, I_CLOSE, "удалить подписку").clicked() {
                                    remove = Some(i);
                                }
                                if icon_button(ui, I_RETRY, "проверить сейчас").clicked() {
                                    check = Some(i);
                                }
                            });
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(ago(sub.last_check)).size(11.5).color(FAINT));
                            ui.add_space(10.0);
                            toggle(ui, &mut sub.audio_only);
                            ui.label(RichText::new("только звук").size(11.5).color(MUTED));
                            ui.add_space(10.0);
                            ui.label(RichText::new("не старше").size(11.5).color(MUTED));
                            let opts: Vec<(u32, String)> = [0u32, 7, 30, 90].iter().map(|d| (*d, if *d == 0 { "всё".into() } else { format!("{d} дн") })).collect();
                            segmented(ui, &mut sub.max_age_days, &opts);
                        });
                    });
                    ui.add_space(6.0);
                }
                if let Some(i) = remove {
                    self.s.subscriptions.remove(i);
                }
                if let Some(i) = check {
                    self.check_subs(Some(i));
                }
                ui.add_space(12.0);
                card(ui, |ui| {
                    row(ui, "проверять при запуске программы", "", |ui| {
                        toggle(ui, &mut self.s.subs_on_start);
                    });
                    divider(ui);
                    let exe = std::env::current_exe().unwrap_or_default().with_file_name("vgd.exe");
                    let cmd = format!("schtasks /create /tn \"Very Good Downloader\" /sc hourly /tr \"\\\"{}\\\" sync\"", exe.display());
                    row(ui, "проверять каждый час, даже без окна", "консольная vgd.exe sync проверяет подписки без окна. кнопка копирует команду для планировщика заданий windows: вставьте её в «командную строку» и нажмите enter. удалить задание: schtasks /delete /tn \"Very Good Downloader\"", |ui| {
                        if small_pill(ui, "скопировать команду").clicked() {
                            ui.ctx().copy_text(cmd.clone());
                            self.status = "команда скопирована — вставьте её в командную строку".into();
                        }
                    });
                });
                ui.add_space(20.0);
            });
        });
        if !same(&before, &self.s) {
            self.save_settings();
        }
    }

    // ---------- расширение ----------

    fn install_extension(&mut self, name: &'static str, browser: &Path, page: &str) {
        let dst = match vgd::appinfo::install_extension_files() {
            Ok(d) => d,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if let Ok(exe) = std::env::current_exe() {
            ipc::register_protocol(&exe); // чтобы кнопка в браузере могла открыть закрытую программу
        }
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(dst.display().to_string());
        }
        let _ = std::process::Command::new(browser).arg(page).spawn();
        // Проводник на уровень выше, с выделенной папкой extension — её сразу видно и можно перетащить.
        reveal(&dst);
        self.ext_steps = Some(name);
    }

    /// Firefox ставит только подписанные Mozilla расширения: открываем .xpi — Firefox сам спросит «Добавить?».
    fn install_firefox(&mut self, browser: &Path) {
        let Some(xpi) = vgd::appinfo::firefox_xpi() else {
            self.status = "расширение для firefox не найдено рядом с программой — переустановите программу".into();
            return;
        };
        if let Ok(exe) = std::env::current_exe() {
            ipc::register_protocol(&exe);
        }
        let _ = std::process::Command::new(browser).arg(&xpi).spawn();
        self.ext_steps = Some("Firefox");
    }

    /// Кнопки браузеров для установки расширения.
    fn browser_buttons(&mut self, ui: &mut egui::Ui) {
        let browsers = vgd::appinfo::browsers();
        let firefox = vgd::appinfo::firefox();
        if browsers.is_empty() && firefox.is_none() {
            ui.label(RichText::new("chrome, edge, яндекс.браузер и firefox не найдены").size(12.0).color(MUTED));
        }
        if let Some(ff) = firefox {
            if small_pill(ui, "Firefox").clicked() {
                self.install_firefox(&ff);
            }
        }
        for (name, exe, page) in browsers.iter().rev() {
            if small_pill(ui, name).clicked() {
                let (name, exe, page) = (*name, exe.clone(), *page);
                self.install_extension(name, &exe, page);
            }
        }
    }

    /// Кнопки «скачать» прямо в приложении Spotify (расширение Spicetify).
    fn ui_spotify_app(&mut self, ui: &mut egui::Ui) {
        use vgd::spotify_app::Status;
        // Проверка папок дешёвая, но не каждый кадр.
        let now = ui.input(|i| i.time);
        let id = egui::Id::unique("spotify-status");
        let cached: Option<(f64, u8)> = ui.data(|d| d.get_temp(id));
        let st = match cached {
            Some((t, s)) if now - t < 3.0 => s,
            _ => {
                let s = vgd::spotify_app::status() as u8;
                ui.data_mut(|d| d.insert_temp(id, (now, s)));
                s
            }
        };
        let st = [Status::NoSpotify, Status::Store, Status::Off, Status::On][st as usize];
        card(ui, |ui| {
            let (text, color) = match st {
                Status::On => ("кнопки на месте", OK),
                Status::Off if self.s.spotify_app => ("кнопки пропали (spotify обновился) — вернутся сами, когда spotify закрыт", WARN),
                Status::Off => ("не установлено", MUTED),
                Status::Store => ("spotify из microsoft store не поддерживается", ERR),
                Status::NoSpotify => ("spotify для windows не найден", MUTED),
            };
            row(ui, "кнопки ↓ в приложении spotify", "в меню правой кнопки у трека, нескольких треков, плейлиста, альбома, исполнителя и «любимых треков»; кнопка рядом с большой play в шапке плейлиста и альбома; кнопка в плеере — текущий трек. плейлисты любой длины: список берётся внутри spotify. работает через spicetify (совместим с spotx): программа сама поставит или обновит его. после обновления spotify кнопки пропадают — программа возвращает их сама при запуске и при входе в windows, пока spotify закрыт.", |ui| {
                ui.label(RichText::new(text).size(12.0).color(color));
            });
            divider(ui);
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 38.0), Layout::left_to_right(Align::Center), |ui| {
                let can = matches!(st, Status::On | Status::Off) && !self.side_task;
                let label = if st == Status::On { "переустановить" } else { "установить в spotify" };
                if small_pill_enabled(ui, label, can).clicked() && can {
                    self.side_task = true;
                    self.status = "ставлю кнопки в spotify (spotify перезапустится)…".into();
                    let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                    std::thread::spawn(move || {
                        let ltx = tx.clone();
                        let r = vgd::spotify_app::install(true, &|m| {
                            let _ = ltx.send(Event::Log(m));
                        });
                        let _ = tx.send(Event::SpotifyDone(r));
                        ctx.request_repaint();
                    });
                }
                if self.s.spotify_app && small_pill(ui, "не следить").on_hover_text("не возвращать кнопки после обновлений spotify").clicked() {
                    self.s.spotify_app = false;
                    if let Ok(exe) = std::env::current_exe() {
                        vgd::spotify_app::set_autorepair(false, &exe);
                    }
                }
            });
        });
    }

    fn ui_extension(&mut self, ui: &mut egui::Ui) {
        let seen = ipc::extension_version().or_else(|| (!self.s.ext_version.is_empty()).then(|| self.s.ext_version.clone()));
        if self.ext_wizard && seen.is_none() {
            // Открыто сразу после установки программы.
            egui::Frame::new().fill(Color32::from_rgb(0x10, 0x16, 0x10)).corner_radius(12).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("поставьте расширение — на сайтах появятся кнопки ↓").size(13.0).color(TEXT));
                ui.add(egui::Label::new(RichText::new("выберите браузер: программа откроет его и подскажет, что нажать. это минута.").size(12.0).color(MUTED)).wrap());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    self.browser_buttons(ui);
                    if link(ui, "позже").clicked() {
                        self.ext_wizard = false;
                    }
                });
            });
            ui.add_space(6.0);
        }
        card(ui, |ui| {
            let status = match &seen {
                Some(v) => format!("подключено · v{v}"),
                None => "не подключено".to_string(),
            };
            row(ui, "кнопки ↓ на youtube, spotify, soundcloud, яндекс музыке и vk", "", |ui| {
                ui.label(RichText::new(status).size(12.0).color(if seen.is_some() { OK } else { MUTED }));
            });
            divider(ui);
            row(ui, "установить в браузер", "chrome, edge и яндекс.браузер не дают программам ставить расширения сами — программа подготовит папку, скопирует её путь и откроет страницу расширений. в firefox расширение ставится одной кнопкой «добавить».", |ui| {
                self.browser_buttons(ui);
            });
        });
        if let Some(name) = self.ext_steps {
            ui.add_space(6.0);
            egui::Frame::new().fill(Color32::from_rgb(0x10, 0x16, 0x10)).corner_radius(12).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let firefox = name == "Firefox";
                ui.label(RichText::new(if firefox { "осталось нажать «добавить» в firefox:".to_string() } else { format!("осталось два щелчка в «{name}»:") }).size(12.5).color(TEXT));
                ui.add_space(4.0);
                let steps: &[&str] = if firefox {
                    &[
                        "1. firefox спросит «добавить very good downloader?» — нажмите «добавить»;",
                        "2. если firefox попросит доступ к сайтам — разрешите, без него кнопки ↓ не появятся;",
                        "3. при первом нажатии ↓ при закрытой программе firefox спросит, чем открыть ссылку vgd:// — выберите very good downloader и отметьте «запомнить».",
                    ]
                } else {
                    &[
                        "1. на открывшейся странице расширений включите «режим разработчика» (переключатель вверху справа);",
                        "2. в открывшемся проводнике уже выделена папка extension — перетащите её на страницу расширений. или нажмите «загрузить распакованное расширение», вставьте путь (ctrl+v — он уже скопирован) и «выбор папки»;",
                        "3. готово — кнопки ↓ появятся на сайтах. если расширение уже было — нажмите у него ↻, чтобы обновить.",
                    ]
                };
                for &step in steps {
                    ui.add(egui::Label::new(RichText::new(step).size(12.0).color(MUTED)).wrap());
                }
                ui.add_space(4.0);
                if link(ui, "понятно, скрыть").clicked() {
                    self.ext_steps = None;
                }
            });
        }
    }

    // ---------- журнал ----------

    fn ui_log(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 16 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("журнал").size(17.0).color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if small_pill_enabled(ui, "очистить", !self.log.is_empty()).clicked() {
                        self.log.clear();
                    }
                    if small_pill_enabled(ui, "копировать", !self.log.is_empty()).clicked() {
                        ui.ctx().copy_text(self.log.join("\n"));
                    }
                });
            });
            ui.add_space(10.0);
            egui::Frame::new().fill(SURFACE).corner_radius(12).inner_margin(12).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.log.is_empty() {
                            ui.label(RichText::new("пока пусто").color(FAINT));
                        }
                        for l in &self.log {
                            let color = if l.starts_with("Ошибка") {
                                ERR
                            } else if l.starts_with("Внимание") || l.starts_with("Предупреждение") {
                                WARN
                            } else if l.starts_with("Скачано") || l.starts_with("Готово") {
                                TEXT
                            } else {
                                MUTED
                            };
                            ui.add(egui::Label::new(RichText::new(l).size(12.0).color(color)).wrap());
                        }
                    });
            });
        });
    }

    // ---------- окна поверх ----------

    fn ui_sort_plan(&mut self, ctx: &egui::Context) {
        let Some(plan) = &self.sort_plan else { return };
        let mut close = false;
        let mut go = false;
        egui::Modal::new(egui::Id::unique("sortplan")).show(ctx, |ui| {
            ui.set_width(760.0);
            ui.label(RichText::new(format!("разложить {} файл(ов)?", plan.len())).size(18.0).color(TEXT));
            ui.add_space(4.0);
            ui.label(RichText::new("сначала посмотрите, куда что переедет").size(12.0).color(MUTED));
            ui.add_space(10.0);
            egui::Frame::new().fill(BG).corner_radius(10).inner_margin(10).show(ui, |ui| {
                egui::ScrollArea::both().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (src, dst) in plan {
                        let rel = dst
                            .parent()
                            .and_then(|p| p.strip_prefix(&self.sort_root).ok())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(rel).size(12.0).color(TEXT));
                            ui.label(RichText::new(format!("←  {name}")).size(12.0).color(MUTED));
                        });
                    }
                });
            });
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, None, "переместить", true, true).clicked() {
                    go = true;
                }
                if pill(ui, None, "отмена", false, true).clicked() {
                    close = true;
                }
            });
        });
        if go {
            let plan = self.sort_plan.take().unwrap_or_default();
            let tx = self.tx.clone();
            let n = apply_plan(&plan, &|m| {
                let _ = tx.send(Event::Log(m));
            });
            self.status = format!("разложено файлов: {n}");
        } else if close {
            self.sort_plan = None;
        }
    }

    /// Редактор категорий: название (= папка) и слова, по которым видео туда попадает.
    fn ui_topics(&mut self, ctx: &egui::Context) {
        let Some(rows) = self.topics_edit.as_mut() else { return };
        let mut close = false;
        let mut save = false;
        let mut remove = None;
        let mut up = None;
        egui::Modal::new(egui::Id::unique("topics")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.label(RichText::new("категории").size(18.0).color(TEXT));
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(
                "категория — это папка в папке загрузок. видео попадает в неё, если в названии, тегах, нике канала или плейлисте есть её слова. \
                 при равенстве выигрывает категория выше. ничего не подошло — рубрика youtube (игры, музыка…) или «разное».",
            ).size(12.0).color(MUTED)).wrap());
            ui.add_space(10.0);
            egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                for (i, (_, name, words)) in rows.iter_mut().enumerate() {
                    egui::Frame::new().fill(SURFACE).corner_radius(10).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(name).hint_text("название").desired_width(200.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, I_CLOSE, "удалить категорию").clicked() {
                                    remove = Some(i);
                                }
                                if i > 0 && small_pill(ui, "выше").clicked() {
                                    up = Some(i);
                                }
                            });
                        });
                        ui.add(
                            egui::TextEdit::multiline(words)
                                .hint_text("слова через запятую: edit, эдит, amv")
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                    });
                    ui.add_space(6.0);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if small_pill(ui, "+ категория").clicked() {
                    rows.push((String::new(), String::new(), String::new()));
                }
                if small_pill(ui, "стандартные").on_hover_text("вернуть категории, которые были при установке").clicked() {
                    *rows = default_topics().into_iter().map(|(n, w)| (String::new(), n, w.join(", "))).collect();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill(ui, None, "сохранить", true, true).clicked() {
                        save = true;
                    }
                    if pill(ui, None, "отмена", false, true).clicked() {
                        close = true;
                    }
                });
            });
        });
        if let Some(i) = remove {
            rows.remove(i);
        }
        if let Some(i) = up {
            rows.swap(i, i - 1);
        }
        if save {
            if let Some(rows) = self.topics_edit.take() {
                self.save_topics_edit(rows);
            }
        } else if close {
            self.topics_edit = None;
        }
    }

    fn ui_confirm_exit(&mut self, ctx: &egui::Context) {
        if !self.confirm_exit {
            return;
        }
        egui::Modal::new(egui::Id::unique("exit")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new("идёт загрузка").size(18.0).color(TEXT));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new("прервать её и выйти? уже скачанное сохранится, остальное докачается при следующем запуске.")
                        .size(12.5)
                        .color(MUTED),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, None, "выйти", true, true).clicked() {
                    {
                        let mut sh = self.shared.lock().unwrap();
                        sh.stop_all = true;
                        sh.queue.clear();
                        for (_, c) in sh.current.clone() {
                            c.cancel();
                        }
                    }
                    self.exiting = true;
                    self.confirm_exit = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if pill(ui, None, "остаться", false, true).clicked() {
                    self.confirm_exit = false;
                }
            });
        });
    }
}

impl App {
    /// Показать окно из трея.
    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}

/// Значок в трее: щелчок — открыть окно; меню — «Открыть», «Выход».
fn make_tray(ctx: &egui::Context) -> (Option<TrayIcon>, Receiver<TrayIconEvent>, Receiver<MenuEvent>, MenuId, MenuId) {
    let (ttx, trx) = channel();
    let (mtx, mrx) = channel();
    let c1 = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e| {
        let _ = ttx.send(e);
        c1.request_repaint();
    }));
    let c2 = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = mtx.send(e);
        c2.request_repaint();
    }));
    let open = MenuItem::new("Открыть", true, None);
    let quit = MenuItem::new("Выход", true, None);
    let menu = Menu::new();
    let _ = menu.append(&open);
    let _ = menu.append(&quit);
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../app.png"))
        .ok()
        .and_then(|ic| tray_icon::Icon::from_rgba(ic.rgba, ic.width, ic.height).ok());
    let mut b = TrayIconBuilder::new().with_tooltip(APP_NAME).with_menu(Box::new(menu)).with_menu_on_left_click(false);
    if let Some(i) = icon {
        b = b.with_icon(i);
    }
    (b.build().ok(), trx, mrx, open.id().clone(), quit.id().clone())
}

impl eframe::App for App {
    /// Вызывается и когда окно спрятано в трей — здесь обрабатываем щелчки по значку.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(e) = self.tray_rx.try_recv() {
            match e {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { .. } => self.show_window(ctx),
                _ => {}
            }
        }
        while let Ok(e) = self.menu_rx.try_recv() {
            if e.id == self.tray_open {
                self.show_window(ctx);
            } else if e.id == self.tray_quit {
                self.show_window(ctx);
                self.exiting = true;
                {
                    let mut sh = self.shared.lock().unwrap();
                    sh.stop_all = true;
                    sh.queue.clear();
                    for (_, c) in sh.current.clone() {
                        c.cancel();
                    }
                }
                self.save_settings();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.hidden {
            self.handle_events(ctx);
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(z) = self.pending_zoom.take() {
            ctx.set_zoom_factor(z.clamp(0.5, 3.0));
        }
        if !self.start_urls.is_empty() {
            self.urls = std::mem::take(&mut self.start_urls).join("\n");
            self.add_and_start(&ctx);
        }
        self.handle_events(&ctx);
        if !self.subs_started {
            self.subs_started = true;
            if self.s.subs_on_start && !self.s.subscriptions.is_empty() {
                self.check_subs(None);
            }
        }
        if self.s.clipboard_watch {
            let now = ctx.input(|i| i.time);
            if now - self.clip_at > 1.0 {
                self.clip_at = now;
                if let Some(t) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()) {
                    let t = t.trim().to_string();
                    if t != self.clip_last {
                        let first = self.clip_last.is_empty();
                        self.clip_last = t.clone();
                        // То, что было в буфере ещё до запуска, не предлагаем.
                        if !first && supported_link(&t) && !self.jobs.iter().any(|j| j.url == t) && self.urls.trim() != t {
                            self.clip_offer = Some(t);
                        }
                    }
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        match ipc::extension_version() {
            Some(v) if v != self.s.ext_version => {
                self.s.ext_version = v;
                self.ext_steps = None;
                self.ext_wizard = false;
                self.save_settings();
            }
            Some(_) => {}
            // Ждём, пока расширение отзовётся (после установки в браузере) — без лишней нагрузки.
            None => ctx.request_repaint_after(std::time::Duration::from_secs(2)),
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            // Размер окна в «обычных» точках (без масштаба интерфейса) — чтобы открыть таким же.
            if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
                let z = ctx.zoom_factor();
                self.s.win_w = (r.width() * z).round();
                self.s.win_h = (r.height() * z).round();
            }
            self.save_settings();
            // Крестик при включённом трее — окно прячется к часам, программа и загрузки продолжают работать
            // (как у Telegram, Discord). Совсем выйти — правой кнопкой по значку → «выход».
            if self.s.tray && self._tray.is_some() && !self.exiting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.hidden = true;
            } else if self.busy() && !self.exiting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_exit = true;
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
            self.add_and_start(&ctx);
        }

        egui::Panel::left("nav")
            .exact_size(76.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).stroke(Stroke::new(1.0, Color32::from_rgb(0x16, 0x16, 0x16))))
            .show(ui, |ui| self.ui_sidebar(ui));
        egui::CentralPanel::default_margins().frame(egui::Frame::new().fill(BG)).show(ui, |ui| match self.page {
            Page::Download => self.ui_download(ui),
            Page::Settings => self.ui_settings(ui),
            Page::Subs => self.ui_subs(ui),
            Page::Log => self.ui_log(ui),
        });

        self.ui_sort_plan(&ctx);
        self.ui_topics(&ctx);
        self.ui_confirm_exit(&ctx);
        if self.busy() || self.side_task {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

// ---------- элементы интерфейса ----------

/// «…\Videos\Very Good Downloader» — две последние части пути.
fn short_dir(dir: &str) -> String {
    let comps: Vec<String> = Path::new(dir).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if comps.len() <= 2 {
        return dir.to_string();
    }
    format!("…\\{}", comps[comps.len() - 2..].join("\\"))
}

fn same(a: &Settings, b: &Settings) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

/// Колонка по центру шириной до COLUMN.
fn centered(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    let w = (avail - 40.0).min(COLUMN);
    let side = ((avail - w) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(w);
            add(ui);
        });
    });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(MUTED)).wrap());
}

fn text_size(ui: &egui::Ui, text: &str, font: &FontId) -> Vec2 {
    ui.painter().layout_no_wrap(text.to_string(), font.clone(), TEXT).size()
}

/// Сегментный переключатель: несколько вариантов в одной плашке, выбранный — светлый.
fn segmented<T: PartialEq + Clone>(ui: &mut egui::Ui, value: &mut T, opts: &[(T, String)]) -> bool {
    let mut changed = false;
    let font = FontId::monospace(13.0);
    let total: f32 = opts.iter().map(|(_, l)| (text_size(ui, l, &font).x + 24.0).max(48.0) + 2.0).sum::<f32>() + 6.0;
    let width = total.min(ui.available_width());
    egui::Frame::new().fill(SURFACE).corner_radius(11).inner_margin(3).show(ui, |ui| {
        ui.set_max_width(width - 6.0);
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        ui.horizontal_wrapped(|ui| {
            // В строках настроек кнопки стоят справа (раскладка справа налево) — порядок сохраняем.
            let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
            let items: Vec<&(T, String)> = if rtl { opts.iter().rev().collect() } else { opts.iter().collect() };
            for (v, label) in items {
                let size = text_size(ui, label, &font) + Vec2::new(24.0, 0.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(size.x.max(48.0), 30.0), Sense::click());
                let selected = v == value;
                let fill = if selected {
                    ACCENT
                } else if resp.hovered() {
                    SURFACE_HI
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, 8, fill);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    font.clone(),
                    if selected { ON_ACCENT } else { TEXT },
                );
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() && !selected {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    });
    changed
}

/// Карточка с несколькими строками настроек.
fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(SURFACE).corner_radius(12).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, Color32::from_rgb(0x20, 0x20, 0x20));
}

/// Строка: подпись слева (подсказка при наведении), элементы управления справа.
fn row(ui: &mut egui::Ui, label: &str, tip: &str, add: impl FnOnce(&mut egui::Ui)) {
    let open_id = egui::Id::unique(("tip", label));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 38.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(38.0);
        ui.label(RichText::new(label).size(13.0).color(TEXT));
        if !tip.is_empty() {
            // Кружок «?»: щелчок показывает/скрывает пояснение под строкой.
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let hot = resp.hovered() || open;
            ui.painter().circle_stroke(rect.center(), 7.5, Stroke::new(1.0, if hot { TEXT } else { FAINT }));
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "?", FontId::monospace(10.5), if hot { TEXT } else { MUTED });
            if resp.clicked() {
                open = !open;
                ui.data_mut(|d| d.insert_temp(open_id, open));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
    if open && !tip.is_empty() {
        ui.add(egui::Label::new(RichText::new(tip).size(11.5).color(MUTED)).wrap());
        ui.add_space(6.0);
    }
}

/// Переключатель-тумблер.
fn toggle(ui: &mut egui::Ui, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui.ctx().animate_bool(resp.id, *value);
    let p = ui.painter();
    p.rect_filled(rect, 10, Color32::from_rgb(0x3a, 0x3a, 0x3a).lerp_to_gamma(ACCENT, t));
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    p.circle_filled(egui::pos2(x, rect.center().y), 7.0, Color32::from_rgb(0x9a, 0x9a, 0x9a).lerp_to_gamma(Color32::BLACK, t));
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

fn small_pill(ui: &mut egui::Ui, text: &str) -> egui::Response {
    small_pill_enabled(ui, text, true)
}

/// Небольшая кнопка для строк настроек.
fn small_pill_enabled(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let font = FontId::monospace(12.0);
    let size = text_size(ui, text, &font) + Vec2::new(22.0, 0.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(size.x, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let p = ui.painter();
    p.rect_filled(rect, 9, if hovered { SURFACE_HI } else { Color32::from_rgb(0x1b, 0x1b, 0x1b) });
    p.rect_stroke(rect, 9, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, if enabled { TEXT } else { FAINT });
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Кнопка-«пилюля»: основная — светлая, остальные — тёмные.
fn pill(ui: &mut egui::Ui, icon: Option<char>, text: &str, primary: bool, enabled: bool) -> egui::Response {
    let font = FontId::monospace(13.5);
    let ifont = FontId::new(14.0, icons());
    let tw = text_size(ui, text, &font).x;
    let iw = if icon.is_some() { 22.0 } else { 0.0 };
    let size = Vec2::new(tw + iw + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let (fill, fg) = match (primary, enabled) {
        (true, true) => (if hovered { Color32::WHITE } else { ACCENT }, ON_ACCENT),
        (false, true) => (if hovered { SURFACE_HI } else { SURFACE }, TEXT),
        (_, false) => (SURFACE, FAINT),
    };
    let p = ui.painter();
    p.rect_filled(rect, 11, fill);
    if !primary {
        p.rect_stroke(rect, 11, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    }
    let mut x = rect.left() + 14.0;
    if let Some(ic) = icon {
        p.text(egui::pos2(x, rect.center().y + 1.0), Align2::LEFT_CENTER, ic, ifont, fg);
        x += iw;
    }
    p.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, font, fg);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Маленькая текстовая ссылка-кнопка.
fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Label::new(RichText::new(text).size(12.0).color(MUTED)).sense(Sense::click()));
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() {
        let y = r.rect.bottom();
        ui.painter().line_segment([egui::pos2(r.rect.left(), y), egui::pos2(r.rect.right(), y)], Stroke::new(1.0, MUTED));
    }
    r
}

/// Значок без рамки (для карточек очереди).
fn icon_button(ui: &mut egui::Ui, icon: char, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8, SURFACE_HI);
    }
    let color = if resp.hovered() { TEXT } else { MUTED };
    if icon == I_FOLDER {
        paint_folder(ui.painter(), rect.center(), color);
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, FontId::new(12.0, icons()), color);
    }
    resp
}

/// Значок папки (в шрифте Segoe MDL2 он плохо читается на мелком размере).
fn paint_folder(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    let body = egui::Rect::from_center_size(c + Vec2::new(0.0, 1.0), Vec2::new(13.0, 9.0));
    p.rect_stroke(body, 2, stroke, StrokeKind::Middle);
    let tab = egui::Rect::from_min_size(body.left_top() - Vec2::new(0.0, 2.5), Vec2::new(5.5, 2.5));
    p.rect_filled(tab, 1, color);
}

fn nav_item(ui: &mut egui::Ui, icon: char, label: &str, selected: bool, badge: Option<usize>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(62.0, 52.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 10, SURFACE_HI);
    } else if resp.hovered() {
        p.rect_filled(rect, 10, SURFACE);
    }
    let fg = if selected || resp.hovered() { TEXT } else { MUTED };
    p.text(rect.center_top() + Vec2::new(0.0, 19.0), Align2::CENTER_CENTER, icon, FontId::new(17.0, icons()), fg);
    p.text(rect.center_bottom() - Vec2::new(0.0, 13.0), Align2::CENTER_CENTER, label, FontId::monospace(11.0), fg);
    if let Some(n) = badge {
        let c = rect.right_top() + Vec2::new(-14.0, 12.0);
        p.circle_filled(c, 8.0, ACCENT);
        p.text(c, Align2::CENTER_CENTER, n.min(99).to_string(), FontId::monospace(10.0), ON_ACCENT);
    }
    resp
}

/// Кратко о пресете: «1080p · до 24 fps · копия h.264 · mp3 320 · папка …».
fn preset_summary(v: &serde_json::Map<String, serde_json::Value>) -> String {
    let s = Settings::default().with_values(v);
    let mut parts = vec![height_short(s.max_height).to_string()];
    if s.max_fps > 0 {
        parts.push(format!("до {} fps", s.max_fps));
    }
    if s.edit != "none" {
        let f = if s.edit == "prores" { "prores" } else { "h.264" };
        parts.push(if s.edit_replace { format!("сразу в {f}") } else { format!("копия {f}") });
    }
    parts.push(format!("музыка: {}", if s.music_format == "best" { "как есть" } else { s.music_format.as_str() }));
    if s.subs {
        parts.push(format!("субтитры {}", s.sub_langs));
    }
    if s.sponsorblock {
        parts.push("sponsorblock".into());
    }
    if s.split_chapters {
        parts.push("главы".into());
    }
    parts.push(format!("папка {}", s.out_dir));
    parts.join(" · ")
}

/// Пресет и свои настройки для ссылки: выбранное значение, совпадающее с пресетом/общими, не запоминается.
/// `next` — панель для следующих ссылок (без фрагмента и «только звук» — они выше).
fn opts_editor(ui: &mut egui::Ui, s: &Settings, o: &mut JobOpts, music: bool, next: bool) -> bool {
    let mut changed = false;
    let base = s.for_job(&JobOpts { preset: o.preset.clone(), ..Default::default() });
    let cur = s.for_job(o);
    card(ui, |ui| {
        if !s.presets.is_empty() {
            row(ui, "пресет", "набор настроек, сохранённый в «настройки → пресеты». «обычные» — текущие настройки программы.", |ui| {
                let mut opts = vec![(String::new(), "обычные".to_string())];
                opts.extend(s.presets.iter().map(|p| (p.name.clone(), p.name.to_lowercase())));
                let mut v = o.preset.clone();
                if segmented(ui, &mut v, &opts) {
                    o.preset = v;
                    o.over.clear();
                    changed = true;
                }
            });
            divider(ui);
        }
        let set = |o: &mut JobOpts, key: &str, v: serde_json::Value, base_v: serde_json::Value| {
            if v == base_v {
                o.over.remove(key);
            } else {
                o.over.insert(key.to_string(), v);
            }
        };
        if music {
            row(ui, "формат музыки", "", |ui| {
                let opts = vec![
                    ("best".to_string(), "как есть".to_string()),
                    ("mp3".to_string(), "mp3 320".to_string()),
                    ("m4a".to_string(), "m4a".to_string()),
                    ("flac".to_string(), "flac".to_string()),
                ];
                let mut v = cur.music_format.clone();
                if segmented(ui, &mut v, &opts) {
                    set(o, "music_format", v.into(), base.music_format.clone().into());
                    changed = true;
                }
            });
        } else {
            row(ui, "качество", "", |ui| {
                let opts: Vec<(u32, String)> = vgd::settings::HEIGHTS.iter().map(|h| (*h, height_short(*h).to_string())).collect();
                let mut v = cur.max_height;
                if segmented(ui, &mut v, &opts) {
                    set(o, "max_height", v.into(), base.max_height.into());
                    changed = true;
                }
            });
            divider(ui);
            row(ui, "макс. fps", "", |ui| {
                let opts: Vec<(u32, String)> = vgd::settings::FPS_LIMITS.iter().map(|f| (*f, if *f == 0 { "макс".to_string() } else { f.to_string() })).collect();
                let mut v = cur.max_fps;
                if segmented(ui, &mut v, &opts) {
                    set(o, "max_fps", v.into(), base.max_fps.into());
                    changed = true;
                }
            });
            divider(ui);
            row(ui, "для монтажа", "", |ui| {
                let mut r = cur.edit_replace;
                if cur.edit != "none" {
                    ui.label(RichText::new("вместо оригинала").size(11.5).color(MUTED));
                    if toggle(ui, &mut r) {
                        set(o, "edit_replace", r.into(), base.edit_replace.into());
                        changed = true;
                    }
                }
                let opts = vec![
                    ("none".to_string(), "нет".to_string()),
                    ("h264".to_string(), "h.264".to_string()),
                    ("prores".to_string(), "prores".to_string()),
                ];
                let mut v = cur.edit.clone();
                if segmented(ui, &mut v, &opts) {
                    set(o, "edit", v.into(), base.edit.clone().into());
                    changed = true;
                }
            });
        }
        divider(ui);
        row(ui, "папка", "", |ui| {
            if o.over.contains_key("out_dir") && small_pill(ui, "как обычно").clicked() {
                o.over.remove("out_dir");
                changed = true;
            }
            if small_pill(ui, "другая…").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&cur.out_dir).pick_folder() {
                    set(o, "out_dir", d.display().to_string().into(), base.out_dir.clone().into());
                    changed = true;
                }
            }
            let w = (ui.available_width() - 8.0).max(40.0);
            ui.allocate_ui_with_layout(Vec2::new(w, 20.0), Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(short_dir(&cur.out_dir)).size(12.0).color(MUTED)).truncate()).on_hover_text(&cur.out_dir);
            });
        });
        if next {
            divider(ui);
            ui.add_space(4.0);
            ui.label(RichText::new("эти настройки — только для ссылок, добавленных сейчас; общие не меняются.").size(11.0).color(FAINT));
            ui.add_space(4.0);
        }
    });
    changed
}

/// Карточка одной ссылки в очереди. `first` / `last` — можно ли сдвинуть вверх / вниз.
fn job_card(ui: &mut egui::Ui, j: &Job, first: bool, last: bool, actions: &mut Vec<Action>) {
    let resp = egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(12)
        .inner_margin(Margin { left: 14, right: 8, top: 10, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (status, color) = match j.state {
                    State::Done => (stage_name("done").to_string(), OK),
                    State::Error => (stage_name("error").to_string(), ERR),
                    State::Cancelled => (stage_name("cancel").to_string(), FAINT),
                    State::Queued => (stage_name("queued").to_string(), MUTED),
                    State::Paused => (stage_name("paused").to_string(), WARN),
                    State::Running => {
                        let mut s = stage_name(&j.stage).to_string();
                        if !j.item.is_empty() {
                            s = format!("{} · {s}", j.item);
                        }
                        (s, if j.stage == "wait" { WARN } else { TEXT })
                    }
                };
                let buttons = match j.state {
                    State::Queued | State::Paused => 5.0,
                    State::Running => 2.0,
                    _ => 3.0,
                };
                let right_w = 20.0 + 26.0 * buttons + text_size(ui, &status, &FontId::monospace(12.0)).x;
                let title_w = (ui.available_width() - right_w).max(80.0);
                ui.allocate_ui_with_layout(Vec2::new(title_w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_width(title_w);
                    let tcolor = if j.state == State::Cancelled { FAINT } else { TEXT };
                    ui.add(egui::Label::new(RichText::new(&j.title).size(13.5).color(tcolor)).truncate())
                        .on_hover_text(&j.url);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match j.state {
                        State::Queued | State::Running | State::Paused => {
                            if icon_button(ui, I_CLOSE, "отменить").clicked() {
                                actions.push(Action::Cancel(j.id));
                            }
                            if j.state == State::Paused {
                                if icon_button(ui, I_PLAY, "продолжить").clicked() {
                                    actions.push(Action::Resume(j.id));
                                }
                            } else if icon_button(ui, I_PAUSE, "пауза — потом продолжится с того же места").clicked() {
                                actions.push(Action::Pause(j.id));
                            }
                            if j.state != State::Running {
                                let tip = if j.opts.preset.is_empty() && j.opts.over.is_empty() {
                                    "свои настройки для этой ссылки"
                                } else {
                                    "свои настройки для этой ссылки (изменены)"
                                };
                                if icon_button(ui, I_SETTINGS, tip).clicked() {
                                    actions.push(Action::EditOpts(j.id));
                                }
                                if !last && icon_button(ui, I_DOWN, "ниже в очереди").clicked() {
                                    actions.push(Action::Move(j.id, 1));
                                }
                                if !first && icon_button(ui, I_UP, "выше в очереди").clicked() {
                                    actions.push(Action::Move(j.id, -1));
                                }
                            }
                        }
                        _ => {
                            if icon_button(ui, I_CLOSE, "убрать из списка").clicked() {
                                actions.push(Action::Remove(j.id));
                            }
                            if icon_button(ui, I_RETRY, "скачать заново (уже скачанное пропустится)").clicked() {
                                actions.push(Action::Retry(j.id));
                            }
                            if let Some(f) = j.files.first() {
                                if icon_button(ui, I_FOLDER, "показать в папке").clicked() {
                                    actions.push(Action::Reveal(f.clone()));
                                }
                            }
                        }
                    }
                    ui.label(RichText::new(status).size(12.0).color(color));
                });
            });
            if j.state == State::Running {
                ui.add_space(8.0);
                let w = ui.available_width() - 6.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 4.0), Sense::hover());
                ui.painter().rect_filled(rect, 2, Color32::from_rgb(0x2c, 0x2c, 0x2c));
                let frac = (j.percent / 100.0).clamp(0.0, 1.0);
                if frac > 0.0 {
                    let mut fill = rect;
                    fill.set_width((rect.width() * frac).max(4.0));
                    ui.painter().rect_filled(fill, 2, if j.stage == "wait" { WARN } else { ACCENT });
                }
                ui.add_space(6.0);
                let mut parts = Vec::new();
                if !j.message.is_empty() {
                    parts.push(j.message.to_lowercase());
                }
                if j.percent > 0.0 && j.percent < 100.0 {
                    parts.push(format!("{:.0}%", j.percent));
                }
                if !j.speed.is_empty() {
                    parts.push(j.speed.clone());
                }
                if !j.eta.is_empty() {
                    parts.push(format!("осталось {}", j.eta));
                }
                ui.add(egui::Label::new(RichText::new(parts.join("  ·  ")).size(11.5).color(MUTED)).truncate());
            } else if !j.error.is_empty() || !j.message.is_empty() {
                ui.add_space(4.0);
                let (t, c) = if j.error.is_empty() { (j.message.as_str(), MUTED) } else { (j.error.as_str(), ERR) };
                ui.add(egui::Label::new(RichText::new(t).size(11.5).color(c)).wrap());
            }
            if !j.notice.is_empty() {
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&j.notice).size(11.5).color(WARN)).wrap());
            }
        });
    let _ = resp;
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// «2 ч назад», «вчера» — для подписок.
fn ago(ts: u64) -> String {
    if ts == 0 {
        return "ещё не проверялась".into();
    }
    let d = now_secs().saturating_sub(ts);
    match d {
        0..=89 => "проверена только что".into(),
        90..=3599 => format!("проверена {} мин назад", d / 60),
        3600..=86399 => format!("проверена {} ч назад", d / 3600),
        _ => format!("проверена {} дн. назад", d / 86400),
    }
}

/// Ссылка, которую имеет смысл предлагать скачать из буфера обмена.
fn supported_link(t: &str) -> bool {
    let t = t.trim();
    if !ipc::is_web_url(t) || t.len() > 500 {
        return false;
    }
    let l = t.to_lowercase();
    vgd::music::is_music_url(&l)
        || [
            "youtube.com/", "youtu.be/", "tiktok.com/", "vk.com/video", "vk.ru/video", "vkvideo.ru/", "rutube.ru/", "pinterest.",
            "pin.it/", "vimeo.com/", "twitch.tv/", "instagram.com/", "x.com/", "twitter.com/", "reddit.com/", "dailymotion.com/",
            "ok.ru/video", "coub.com/", "bilibili.com/",
        ]
        .iter()
        .any(|h| l.contains(h))
}

fn info(title: &str, text: &str) {
    rfd::MessageDialog::new().set_title(title).set_description(text).set_level(rfd::MessageLevel::Info).show();
}

/// Рабочий поток: берёт ссылки из очереди по одной. `side` — вторая нить: только небольшая музыка.
fn work(shared: Arc<Mutex<Shared>>, tx: Sender<Event>, ctx: egui::Context, side: bool) {
    loop {
        let (id, url, settings, opts) = {
            let mut sh = shared.lock().unwrap();
            if sh.stop_all {
                break;
            }
            let pos = if side {
                sh.queue.iter().position(|(_, u, _)| vgd::music::is_small_music(u))
            } else if sh.queue.is_empty() {
                None
            } else {
                Some(0)
            };
            match pos.and_then(|i| sh.queue.remove(i)) {
                // Общие настройки → пресет → свои значения ссылки.
                Some((id, url, opts)) => (id, url, sh.settings.for_job(&opts), opts),
                None => break,
            }
        };
        let (ptx, pctx, ltx, lctx) = (tx.clone(), ctx.clone(), tx.clone(), ctx.clone());
        let mut dl = Downloader::new(
            settings,
            Arc::new(move |p: &Progress| {
                let _ = ptx.send(Event::Progress(id, p.clone()));
                pctx.request_repaint();
            }),
            Arc::new(move |m: String| {
                let _ = ltx.send(Event::Log(m));
                lctx.request_repaint();
            }),
        );
        dl.opts = opts;
        shared.lock().unwrap().current.push((id, dl.cancel_handle()));
        let ev = match dl.run(&url) {
            Ok(files) => Event::Finish(id, State::Done, files, None),
            Err(DlError::Cancelled(m)) => {
                if !m.is_empty() {
                    let _ = tx.send(Event::Log(m.clone()));
                }
                Event::Finish(id, State::Cancelled, Vec::new(), if m.is_empty() { None } else { Some(m) })
            }
            Err(DlError::Failed(m)) => {
                let _ = tx.send(Event::Log(format!("Ошибка ({url}): {m}")));
                Event::Finish(id, State::Error, Vec::new(), Some(m))
            }
        };
        shared.lock().unwrap().current.retain(|(cid, _)| *cid != id);
        let _ = tx.send(ev);
        ctx.request_repaint();
    }
    let _ = tx.send(Event::Idle);
    ctx.request_repaint();
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dir = PathBuf::from(windir).join("Fonts");
    // Моноширинный шрифт для всего интерфейса: Cascadia Mono (Windows 11 / Terminal), иначе Consolas.
    let mono = ["CascadiaMono.ttf", "consola.ttf"].iter().find_map(|f| std::fs::read(dir.join(f)).ok());
    if let Some(bytes) = mono {
        fonts.font_data.insert("mono".into(), Arc::new(FontData::from_owned(bytes)));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().insert(0, "mono".into());
        }
    }
    // Запасные шрифты для названий на японском, китайском, корейском и значков вроде 【】★♪ —
    // иначе вместо них квадратики. Берутся только если моноширинному не хватает знака.
    for (key, files) in [
        ("jp", &["YuGothR.ttc", "YuGothM.ttc", "msgothic.ttc"][..]),
        ("zh", &["msyh.ttc", "simsun.ttc"][..]),
        ("ko", &["malgun.ttf"][..]),
        ("sym", &["seguisym.ttf"][..]),
    ] {
        if let Some(bytes) = files.iter().find_map(|f| std::fs::read(dir.join(f)).ok()) {
            fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(key.into());
            }
        }
    }
    if let Ok(bytes) = std::fs::read(dir.join("segmdl2.ttf")) {
        fonts.font_data.insert("icons".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.insert(icons(), vec!["icons".into()]);
    } else {
        // Без значков: пусть хотя бы не будет ошибки шрифта.
        let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        fonts.families.insert(icons(), fallback);
    }
    ctx.set_fonts(fonts);

    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::monospace(11.5)),
            (TextStyle::Body, FontId::monospace(13.5)),
            (TextStyle::Button, FontId::monospace(13.5)),
            (TextStyle::Heading, FontId::monospace(20.0)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.interaction.tooltip_delay = 0.15;
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = Color32::from_rgb(0x10, 0x10, 0x10);
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(16);
        v.menu_corner_radius = CornerRadius::same(10);
        v.extreme_bg_color = Color32::from_rgb(0x0d, 0x0d, 0x0d);
        v.faint_bg_color = SURFACE;
        v.override_text_color = None;
        v.selection.bg_fill = Color32::from_rgb(0x44, 0x44, 0x44);
        v.selection.stroke = Stroke::new(1.0, TEXT);
        v.hyperlink_color = TEXT;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(10);
            w.bg_stroke = Stroke::new(1.0, BORDER);
        }
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.noninteractive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.bg_fill = SURFACE;
        v.widgets.inactive.weak_bg_fill = SURFACE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.bg_fill = SURFACE_HI;
        v.widgets.hovered.weak_bg_fill = SURFACE_HI;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
        v.widgets.active.bg_fill = ACCENT;
        v.widgets.active.weak_bg_fill = ACCENT;
        v.widgets.active.fg_stroke = Stroke::new(1.0, ON_ACCENT);
        v.text_cursor.stroke = Stroke::new(2.0, TEXT);
    });
}

fn main() -> eframe::Result {
    // Окно уже открыто — отдаём ему ссылки (или просто показываем его) и выходим.
    // Окно входа в VK и невидимый браузер для поиска в VK (отдельные процессы, см. vk_web.rs).
    if let Some(login) = std::env::args().find_map(|a| match a.as_str() {
        "--vk-login" => Some(true),
        "--vk-agent" => Some(false),
        _ => None,
    }) {
        if let Err(e) = vgd::vk_web::run_window(login) {
            eprintln!("{e}");
        }
        return Ok(());
    }
    // При входе в Windows: вернуть кнопки в Spotify, если он обновился, — без окна.
    if std::env::args().any(|a| a == "--spotify-repair") {
        if let Some(Err(e)) = vgd::spotify_app::repair(&|_| {}) {
            eprintln!("{e}");
        }
        return Ok(());
    }
    let urls = ipc::launch_urls(std::env::args().skip(1));
    if ipc::forward(&urls) {
        return Ok(());
    }
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../app.png")).ok();
    let saved = Settings::load();
    let size = if saved.win_w >= 560.0 && saved.win_h >= 420.0 { [saved.win_w, saved.win_h] } else { [780.0, 600.0] };
    let mut vp = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size(size)
        .with_min_inner_size([560.0, 420.0]);
    if let Some(icon) = icon {
        vp = vp.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

trait IfEmptyStr {
    fn if_empty(&self, other: &str) -> String;
}

impl IfEmptyStr for str {
    fn if_empty(&self, other: &str) -> String {
        if self.is_empty() { other.to_string() } else { self.to_string() }
    }
}
