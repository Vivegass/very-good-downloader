//! Окно программы: минималистичный тёмный интерфейс — ссылка, очередь, настройки, журнал.
#![windows_subsystem = "windows"]

use eframe::egui;
use egui::{
    Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, RichText,
    Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use vgd::appinfo::{open_path, reveal, APP_NAME, APP_VERSION};
use vgd::ipc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use vgd::downloader::{plan_existing, update_ytdlp, ytdlp_version};
use vgd::queue::{Event as QEvent, Job, Outcome, Queue, State, TicketState};
use vgd::settings::{JobOpts, Settings, Subscription, BROWSERS};
use vgd::theme::{self, Rgb, Theme};
use vgd::sort::{apply_plan, default_topics, load_topics, save_topics, Topics};

#[path = "gui/appearance.rs"]
mod appearance;
#[path = "gui/update_ui.rs"]
mod update_ui;

// ---------- палитра ----------

// Цвета берутся из текущей темы (vgd::theme): тёмная по умолчанию, светлая, свои из папки themes.

#[derive(Clone, Copy)]
struct Pal {
    bg: Color32,
    sidebar: Color32,
    sidebar_line: Color32,
    surface: Color32,
    surface_hi: Color32,
    pill: Color32,
    border: Color32,
    divider: Color32,
    outline: Color32,
    window: Color32,
    field: Color32,
    selection: Color32,
    text: Color32,
    text_hover: Color32,
    muted: Color32,
    faint: Color32,
    accent: Color32,
    accent_hover: Color32,
    on_accent: Color32,
    toggle_off: Color32,
    knob_off: Color32,
    knob_on: Color32,
    track: Color32,
    ok: Color32,
    ok_bg: Color32,
    err: Color32,
    warn: Color32,
    warn_bg: Color32,
    /// множитель скругления углов
    radius: f32,
    /// значки обычными символами вместо Segoe MDL2
    symbols: bool,
    lay: Lay,
}

/// Раскладка и форма из темы (layout), в удобном для рисования виде.
#[derive(Clone, Copy)]
struct Lay {
    /// 0 — слева, 1 — сверху, 2 — снизу
    nav: u8,
    nav_labels: bool,
    nav_width: f32,
    /// 0 — значок программы, 1 — значок и название, 2 — ничего
    header: u8,
    column: f32,
    /// 0 — заливка, 1 — рамка, 2 — без фона
    cards: u8,
    /// 0 — нет, 1 — тень, 2 — свечение
    shadow: u8,
    density: f32,
    padding: f32,
    /// 0 — скруглённые, 1 — прямоугольные, 2 — контур, 3 — плоские, 4 — тональные (Material 3)
    buttons: u8,
    /// скругление кнопок / поля для ссылок в точках; < 0 — по radius
    btn_r: f32,
    field_r: f32,
    /// выбранный пункт навигации — «таблетка» вокруг значка (Navigation Rail)
    nav_pill: bool,
    /// тумблеры в стиле Material 3
    m3_switch: bool,
    bg: [Color32; 3],
    bg_n: usize,
    image_opacity: f32,
    bold: bool,
    upper: bool,
    light: bool,
}

fn idx(v: &str, opts: &[&str]) -> u8 {
    opts.iter().position(|o| *o == v).unwrap_or(0) as u8
}

impl Lay {
    fn of(t: &Theme) -> Self {
        let l = &t.layout;
        let mut bg = [rgb(t.bg); 3];
        for (i, c) in l.background.iter().take(3).enumerate() {
            bg[i] = rgb(*c);
        }
        Lay {
            nav: idx(&l.nav, &["left", "top", "bottom"]),
            nav_labels: l.nav_labels,
            nav_width: l.nav_width,
            header: idx(&l.header, &["logo", "name", "none"]),
            column: l.column,
            cards: idx(&l.cards, &["fill", "outline", "none"]),
            shadow: idx(&l.shadow, &["none", "shadow", "glow"]),
            density: l.density,
            padding: l.padding,
            buttons: idx(&l.buttons, &["pill", "rect", "outline", "flat", "tonal"]),
            btn_r: l.button_radius,
            field_r: l.field_radius,
            nav_pill: l.nav_indicator == "pill",
            m3_switch: l.switch == "m3",
            bg,
            bg_n: l.background.len().min(3),
            image_opacity: l.image_opacity,
            bold: l.bold_headings,
            upper: l.caps == "upper",
            light: t.is_light(),
        }
    }
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0[0], c.0[1], c.0[2])
}

impl Pal {
    fn of(t: &Theme) -> Self {
        Pal {
            bg: rgb(t.bg),
            sidebar: rgb(t.sidebar),
            sidebar_line: rgb(t.sidebar_line),
            surface: rgb(t.surface),
            surface_hi: rgb(t.surface_hi),
            pill: rgb(t.pill),
            border: rgb(t.border),
            divider: rgb(t.divider),
            outline: rgb(t.outline),
            window: rgb(t.window),
            field: rgb(t.field),
            selection: rgb(t.selection),
            text: rgb(t.text),
            text_hover: rgb(t.text_hover),
            muted: rgb(t.muted),
            faint: rgb(t.faint),
            accent: rgb(t.accent),
            accent_hover: rgb(t.accent_hover),
            on_accent: rgb(t.on_accent),
            toggle_off: rgb(t.toggle_off),
            knob_off: rgb(t.knob_off),
            knob_on: rgb(t.knob_on),
            track: rgb(t.track),
            ok: rgb(t.ok),
            ok_bg: rgb(t.ok_bg),
            err: rgb(t.err),
            warn: rgb(t.warn),
            warn_bg: rgb(t.warn_bg),
            radius: t.radius,
            symbols: t.icons == "symbols",
            lay: Lay::of(t),
        }
    }
}

thread_local! {
    static PAL: std::cell::Cell<Pal> = std::cell::Cell::new(Pal::of(&theme::dark()));
}

/// Цвета текущей темы.
fn pal() -> Pal {
    PAL.with(|p| p.get())
}

/// Скругление с учётом темы.
fn rad(r: u8) -> u8 {
    (r as f32 * pal().radius).round().clamp(0.0, 255.0) as u8
}

/// Скругление кнопок: у прямоугольных — почти без него.
fn btn_rad(r: u8) -> u8 {
    let lay = pal().lay;
    if lay.btn_r >= 0.0 {
        lay.btn_r.round().min(255.0) as u8
    } else if lay.buttons == 1 {
        rad(r).min(3)
    } else {
        rad(r)
    }
}

/// Слой наведения (как state layer в Material 3): цвет чуть ближе к цвету текста.
fn hover_of(c: Color32) -> Color32 {
    c.lerp_to_gamma(pal().text, 0.08)
}

/// Отступ по горизонтали: множитель темы, при скруглении больше обычного — шире, чтобы текст не прилипал к углам.
fn pad(x: f32) -> f32 {
    let p = pal();
    x * p.lay.padding * (1.0 + (p.radius - 1.0).max(0.0) * 0.5)
}

/// Высота строк и кнопок с учётом плотности.
fn dens(x: f32) -> f32 {
    (x * pal().lay.density).round()
}

/// Внутренние поля карточек и плашек.
fn margin(x: f32, y: f32) -> Margin {
    let p = pal();
    Margin::symmetric(pad(x).round().clamp(0.0, 120.0) as i8, (y * p.lay.padding * p.lay.density).round().clamp(0.0, 120.0) as i8)
}

/// Тень или свечение под карточками.
fn card_shadow() -> egui::Shadow {
    let p = pal();
    match p.lay.shadow {
        1 => egui::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(if p.lay.light { 28 } else { 110 }) },
        2 => egui::Shadow { offset: [0, 0], blur: 16, spread: 0, color: p.accent.gamma_multiply(0.28) },
        _ => egui::Shadow::NONE,
    }
}

/// Заголовок страницы.
fn heading(text: &str, size: f32) -> RichText {
    let r = RichText::new(text).size(size).color(pal().text);
    if pal().lay.bold { r.family(bold()) } else { r }
}

/// Фон окна: градиент сверху вниз и картинка из темы.
fn paint_background(ui: &egui::Ui) {
    let p = pal();
    let rect = ui.max_rect();
    if p.lay.bg_n >= 2 {
        let mut m = egui::Mesh::default();
        let n = p.lay.bg_n;
        for i in 0..n {
            let y = egui::lerp(rect.top()..=rect.bottom(), i as f32 / (n - 1) as f32);
            m.colored_vertex(egui::pos2(rect.left(), y), p.lay.bg[i]);
            m.colored_vertex(egui::pos2(rect.right(), y), p.lay.bg[i]);
            if i > 0 {
                let k = (i * 2) as u32;
                m.add_triangle(k - 2, k - 1, k);
                m.add_triangle(k - 1, k, k + 1);
            }
        }
        ui.painter().add(egui::Shape::mesh(m));
    }
    BG_IMG.with(|b| {
        if let Some(tex) = &b.borrow().1 {
            // как «cover»: заполнить всё, лишнее обрезать
            let [w, h] = tex.size();
            let (ia, ra) = (w as f32 / h.max(1) as f32, rect.width() / rect.height().max(1.0));
            let uv = if ia > ra {
                let f = ra / ia;
                egui::Rect::from_min_max(egui::pos2((1.0 - f) / 2.0, 0.0), egui::pos2((1.0 + f) / 2.0, 1.0))
            } else {
                let f = ia / ra;
                egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - f) / 2.0), egui::pos2(1.0, (1.0 + f) / 2.0))
            };
            ui.painter().image(tex.id(), rect, uv, Color32::WHITE.gamma_multiply(p.lay.image_opacity));
        }
    });
}

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

/// Значок с учётом темы: в наборе «символы» — обычные знаки вместо Segoe MDL2.
fn glyph(icon: char) -> char {
    if !pal().symbols {
        return icon;
    }
    match icon {
        I_DOWNLOAD => '↓',
        I_SETTINGS => '⚙',
        I_LOG => '☰',
        I_LINK => '🔗',
        I_PASTE => '📋',
        I_FOLDER => '📁',
        I_CLOSE => '✕',
        I_RETRY => '↻',
        I_SUBS => '★',
        I_PAUSE => '⏸',
        I_PLAY => '▶',
        I_UP => '▲',
        I_DOWN => '▼',
        other => other,
    }
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

enum Event {
    /// от рабочих нитей очереди (vgd::queue)
    Queue(QEvent),
    Log(String),
    Status(String),
    SortPlan(Vec<(PathBuf, PathBuf)>),
    YtdlpVersion(String),
    /// ссылки из браузера, Spotify или второго запуска программы; «показать окно»
    External(ipc::Incoming),
    /// установка кнопок в Spotify закончилась: успех / текст ошибки
    SpotifyDone(Result<(), String>),
    /// вход в Яндекс: код и страница (Some) или вход закончился (None)
    YandexCode(Option<(String, String)>),
    /// обновление программы: проверка, прогресс скачивания (gui/update_ui.rs)
    Update(update_ui::UpdEvent),
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
    /// очередь загрузок (общая с Android, vgd::queue): карточки, нити, пауза, порядок
    q: Queue,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    /// карточка, у которой открыты свои настройки
    edit_job: Option<u64>,
    /// поле «название нового пресета»
    preset_new: String,
    /// вход в Яндекс идёт: код и страница для ввода
    ya_code: Option<(String, String)>,
    log: Vec<String>,
    /// журнал: строка поиска и «поставить в неё курсор» (Ctrl+F)
    log_find: String,
    log_find_focus: bool,
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
    /// текущая тема оформления и редактор тем (gui/appearance.rs)
    theme: Theme,
    themes: appearance::ThemeUi,
    /// заявки из браузера/Spotify (`POST /add`): номер → карточки; их состояние отдаётся по `/status`
    tickets: Vec<(u64, Vec<u64>)>,
    /// запуск в фоне при входе в Windows (запись в реестре)
    autostart: bool,
    /// запущено в фоне: окно создано за краем экрана (eframe всё равно показывает его после первого кадра —
    /// прячем на первых кадрах), при первом показе — поставить по центру
    offscreen: bool,
    hide_frames: u8,
    /// обновление программы (gui/update_ui.rs)
    upd: update_ui::UpdateUi,
}

impl App {
    /// `inbox` — ссылки, принятые портом ещё до создания окна (порт занимает `main`);
    /// `background` — запуск без окна (`--background`, из расширения или Spotify).
    fn new(cc: &eframe::CreationContext<'_>, inbox: Receiver<ipc::Incoming>, background: bool) -> Self {
        let (tx, rx) = channel();
        // Масштаб только из настроек: Ctrl+колесо/Ctrl+= в egui отключены, чтобы не путались с сохранённым.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        // Ссылки из расширения браузера, Spotify и второго запуска — и те, что пришли, пока окно создавалось.
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for m in inbox {
                    let _ = tx.send(Event::External(m));
                    ctx.request_repaint();
                }
            });
        }
        if let Ok(exe) = std::env::current_exe() {
            vgd::appinfo::fix_autostart(&exe);
        }
        let mut s = Settings::load();
        // Своей темы больше нет в папке — тёмная.
        if let Some(stem) = s.theme.strip_prefix(theme::FILE_PREFIX) {
            if !theme::dir().join(format!("{stem}.json")).is_file() {
                s.theme.clear();
            }
        }
        let theme = theme::load(&s.theme);
        apply_theme(&cc.egui_ctx, &theme);
        // инструкция по темам — в папку тем (только если её нет или она старее)
        std::thread::spawn(|| {
            let _ = theme::guide::ensure(&theme::dir());
        });
        cc.egui_ctx.set_zoom_factor(s.ui_scale.clamp(0.5, 3.0));
        let logo = eframe::icon_data::from_png_bytes(include_bytes!("../../../app.png")).ok().map(|ic| {
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
        // Без значка в трее спрятанное окно было бы не открыть — тогда показываем.
        let start_hidden = background && tray.0.is_some();

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
            q: {
                let (tx, ctx) = (tx.clone(), cc.egui_ctx.clone());
                Queue::new(
                    s.clone(),
                    Arc::new(move |e| {
                        let _ = tx.send(Event::Queue(e));
                        ctx.request_repaint();
                    }),
                )
            },
            s,
            urls: String::new(),
            tx,
            rx,
            edit_job: None,
            preset_new: String::new(),
            ya_code: None,
            log: Vec::new(),
            log_find: String::new(),
            log_find_focus: false,
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
            hidden: start_hidden,
            pending_zoom: None,
            ext_steps: None,
            ext_wizard,
            topics_edit: None,
            show_folders: false,
            theme,
            themes: Default::default(),
            tickets: Vec::new(),
            autostart: vgd::appinfo::autostart(),
            offscreen: background,
            hide_frames: 0,
            upd: Default::default(),
        }
    }

    /// Идёт загрузка (в любой из двух нитей).
    fn busy(&self) -> bool {
        self.q.busy()
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
        self.q.set_settings(self.s.clone());
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
    fn enqueue(&mut self, url: String, opts: JobOpts, sub_url: Option<String>) -> u64 {
        let title = match &sub_url {
            Some(_) => self.s.subscriptions.iter().find(|s| s.url == url).map(|s| s.name.clone()).filter(|n| !n.is_empty()),
            None => None,
        };
        let id = self.q.add(url, opts, sub_url, title);
        self.after_sync();
        id
    }

    /// Очередь для рабочих нитей — ссылки «в очереди» в порядке карточек.
    fn sync_queue(&mut self) {
        self.q.sync();
        self.after_sync();
    }

    fn after_sync(&mut self) {
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
            if self.q.jobs.iter().any(|j| j.sub_url.as_deref() == Some(&sub.url) && matches!(j.state, State::Queued | State::Running | State::Paused)) {
                continue;
            }
            let opts = JobOpts { audio_only: sub.audio_only, max_age_days: sub.max_age_days, seed_only: !sub.seeded, ..Default::default() };
            self.enqueue(sub.url.clone(), opts, Some(sub.url.clone()));
        }
        let ctx = self.ctx.clone();
        self.start_worker(&ctx);
    }

    fn start_worker(&mut self, _ctx: &egui::Context) {
        self.q.start();
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::Cancel(id) => {
                if self.q.cancel(id) {
                    self.status = "отменяю загрузку…".into();
                }
                self.after_sync();
            }
            // Пауза: ссылка остаётся на своём месте, «продолжить» докачает с того же места.
            Action::Pause(id) => {
                self.q.pause(id);
                self.after_sync();
            }
            Action::Resume(id) => {
                self.q.resume(id);
                self.after_sync();
            }
            Action::Move(id, dir) => {
                self.q.move_job(id, dir);
                self.after_sync();
            }
            Action::EditOpts(id) => {
                self.edit_job = if self.edit_job == Some(id) { None } else { Some(id) };
            }
            Action::Remove(id) => self.q.remove(id),
            // Повторить отменённую или упавшую загрузку: скачанное раньше пропустится по архиву.
            Action::Retry(id) => {
                self.q.retry(id);
                self.after_sync();
            }
            Action::Reveal(f) => reveal(Path::new(&f)),
        }
    }

    fn clear_done(&mut self) {
        self.q.clear_done();
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
        let font = if pal().lay.bold { FontId::new(11.5, bold()) } else { FontId::monospace(11.5) };
        let shown = if pal().lay.upper { title.to_uppercase() } else { title.to_string() };
        let size = text_size(ui, &shown, &font) + Vec2::new(18.0, 4.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if resp.hovered() { pal().text } else { pal().muted };
        // треугольник: ▾ — открыта, ▸ — свёрнута
        let c = egui::pos2(rect.left() + 5.0, rect.center().y);
        let pts = if open {
            vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
        } else {
            vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.0, 4.0)]
        };
        ui.painter().add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        ui.painter().text(egui::pos2(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, shown, font, color);
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
        self.status = "ищу скачанные видео и узнаю их каналы…".into();
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
                Event::Queue(ev) => match self.q.handle(ev) {
                    Outcome::Log(m) => self.add_log(m),
                    Outcome::Finished(id) => self.on_finished(id),
                    Outcome::Idle => {
                        let active = self.q.active();
                        if !active && !self.q.jobs.iter().any(|j| j.state == State::Error) && !self.q.jobs.is_empty() {
                            self.status = "все загрузки завершены".into();
                        }
                        if !active && self.status.starts_with("отменяю") {
                            self.status.clear();
                        }
                    }
                    Outcome::Progress(_) => {}
                },
                Event::Log(m) => self.add_log(m),
                Event::Update(e) => self.on_update_event(e),
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
                Event::External(ipc::Incoming::Show) => self.show_window(ctx),
                // Ссылки из браузера / Spotify: сразу в очередь, окно не показываем (спрятано — остаётся
                // в трее, открыто — карточка просто появляется в списке). Об окончании скажет уведомление.
                Event::External(ipc::Incoming::Add { urls, ticket }) => {
                    if !urls.is_empty() {
                        let n = urls.len();
                        let opts = self.next_opts.clone();
                        let ids: Vec<u64> = urls.into_iter().map(|url| self.enqueue(url, opts.clone(), None)).collect();
                        if let Some(t) = ticket {
                            self.tickets.push((t, ids));
                        }
                        self.start_worker(ctx);
                        self.add_log(format!("Из браузера: ссылок {n}"));
                    }
                }
                Event::SortPlan(plan) => {
                    self.side_task = false;
                    if plan.is_empty() {
                        self.status = "раскладывать нечего".into();
                        info("Разложить", "В корне папки нет неразложенных видео, скачанных программой.");
                    } else {
                        self.status = format!("найдено файлов для раскладки: {}", plan.len());
                        self.sort_plan = Some(plan);
                    }
                }
            }
        }
        self.update_tickets();
    }

    /// Загрузка закончилась: подписка запоминает проверку, уведомление Windows.
    fn on_finished(&mut self, id: u64) {
        let job = self.q.job(id).map(|j| (j.files.clone(), j.title.clone(), j.sub_url.clone(), j.state, j.error.clone()));
        let Some((files, title, sub_url, st, err)) = job else { return };
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
        // Окно спрятано — без уведомления об ошибке никто бы не узнал, что не скачалось.
        if self.s.notify_done && st == State::Error && self.hidden {
            vgd::appinfo::toast("Не скачалось", &format!("{title} · {err}"), Path::new(&self.s.out_dir));
        }
    }

    /// Состояние заявок из браузера/Spotify для `GET /status`: пока что-то ждёт или качается — `queued`/`running`,
    /// потом `done` (что-то скачалось) или `error`. Законченные заявки больше не пересчитываем.
    fn update_tickets(&mut self) {
        if self.tickets.is_empty() {
            return;
        }
        let q = &self.q;
        self.tickets.retain(|(t, ids)| match q.ticket_state(ids) {
            TicketState::Running(p) => {
                ipc::set_ticket(*t, "running", &format!("{p:.0}%"));
                true
            }
            TicketState::Queued => {
                ipc::set_ticket(*t, "queued", "");
                true
            }
            TicketState::Done(m) => {
                ipc::set_ticket(*t, "done", &m);
                false
            }
            TicketState::Error(m) => {
                ipc::set_ticket(*t, "error", &m);
                false
            }
        });
    }

    // ---------- боковая панель ----------

    /// Пункты навигации: страница, значок, подпись, число в кружке.
    fn nav_pages(&self) -> [(Page, char, &'static str, Option<usize>); 4] {
        let running = self.q.jobs.iter().filter(|j| matches!(j.state, State::Queued | State::Running)).count();
        let badge = (running > 0).then_some(running);
        [
            (Page::Download, I_DOWNLOAD, "скачать", badge),
            (Page::Subs, I_SUBS, "подписки", None),
            (Page::Settings, I_SETTINGS, "настройки", None),
            (Page::Log, I_LOG, "журнал", None),
        ]
    }

    /// Значок программы и (в теме с header = "name") её название.
    fn nav_header(&self, ui: &mut egui::Ui, size: f32, two_lines: bool) {
        let lay = pal().lay;
        if lay.header == 2 {
            return;
        }
        if let Some(logo) = &self.logo {
            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(size)));
        }
        if lay.header == 1 {
            let t = RichText::new(if two_lines { "very good
downloader" } else { "very good downloader" }).size(12.5).color(pal().text);
            ui.label(if lay.bold { t.family(bold()) } else { t });
        }
    }

    /// Навигация полосой сверху или снизу.
    fn ui_navbar(&mut self, ui: &mut egui::Ui) {
        let lay = pal().lay;
        let pages = self.nav_pages();
        let font = FontId::monospace(12.0);
        let widths: Vec<f32> = pages.iter().map(|(_, _, l, _)| nav_row_width(ui, l, lay.nav_labels, &font)).collect();
        let total: f32 = widths.iter().sum::<f32>() + 4.0 * (pages.len() as f32 - 1.0);
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let start = ui.cursor().left();
            self.nav_header(ui, 22.0, false);
            // без названия слева — пункты по центру полосы
            let used = ui.cursor().left() - start;
            let full = ui.available_width() + used;
            let gap = if lay.header == 2 || lay.nav == 2 { ((full - total) / 2.0 - used).max(12.0) } else { 18.0 };
            ui.add_space(gap);
            ui.spacing_mut().item_spacing.x = 4.0;
            for ((page, icon, label, badge), w) in pages.into_iter().zip(widths) {
                if nav_row(ui, icon, label, self.page == page, badge, w, lay.nav_labels).clicked() {
                    self.page = page;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(format!("v{APP_VERSION}")).size(10.5).color(pal().faint));
            });
        });
    }

    fn ui_sidebar(&mut self, ui: &mut egui::Ui) {
        let lay = pal().lay;
        let wide = lay.nav_width >= 140.0;
        ui.add_space(12.0);
        if wide {
            // широкая панель: значок и подпись в строку
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                self.nav_header(ui, 24.0, true);
            });
            ui.add_space(14.0);
            let w = ui.available_width() - 16.0;
            for (page, icon, label, badge) in self.nav_pages() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    if nav_row(ui, icon, label, self.page == page, badge, w, true).clicked() {
                        self.page = page;
                    }
                });
                ui.add_space(2.0);
            }
        } else {
            ui.vertical_centered(|ui| {
                if lay.header != 2 {
                    if let Some(logo) = &self.logo {
                        ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(28.0)));
                    }
                    ui.add_space(14.0);
                }
                for (page, icon, label, badge) in self.nav_pages() {
                    let r = if lay.nav_labels {
                        nav_item(ui, icon, label, self.page == page, badge)
                    } else {
                        nav_row(ui, icon, label, self.page == page, badge, 44.0, false)
                    };
                    if r.clicked() {
                        self.page = page;
                    }
                    ui.add_space(2.0);
                }
            });
        }
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("v{APP_VERSION}")).size(10.5).color(pal().faint));
        });
    }

    // ---------- скачать ----------

    fn ui_download(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().id_salt("download").auto_shrink([false, false]).show(ui, |ui| {
            centered(ui, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.label(heading("very good downloader", 17.0));
                    ui.label(RichText::new("youtube · spotify · apple music · soundcloud · яндекс музыка · vk…").size(11.5).color(pal().faint));
                });
                ui.add_space(10.0);
                self.ui_update_banner(ui);

                // поле для ссылок
                let focused_id = egui::Id::unique("urls");
                let has_focus = ui.memory(|m| m.has_focus(focused_id));
                egui::Frame::new()
                    .fill(pal().surface)
                    .stroke(Stroke::new(1.0, if has_focus { pal().muted } else { pal().border }))
                    .corner_radius(if pal().lay.field_r >= 0.0 { pal().lay.field_r.round() as u8 } else { rad(12) })
                    .inner_margin(margin(12.0, 9.0))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(glyph(I_LINK).to_string()).family(icons()).size(15.0).color(pal().muted));
                            ui.add(
                                egui::TextEdit::multiline(&mut self.urls)
                                    .id(focused_id)
                                    .frame(egui::Frame::NONE)
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(
                                        RichText::new("вставь ссылку — несколько можно с новой строки").color(pal().faint),
                                    ),
                            );
                        });
                    });
                ui.add_space(8.0);

                // Музыкальная ссылка в поле — вместо качества видео показываем формат звука.
                let music_mode = self.urls.lines().any(|l| vgd::music::is_music_url(l.trim()));
                let spotify = self.urls.to_lowercase().contains("spotify");
                ui.horizontal(|ui| {
                    // место под кнопки справа — переключатель качества переносится, а не наезжает на них
                    let reserve = pill_width(ui, "скачать") + pill_width(ui, "вставить") + dens(34.0) + 3.0 * ui.spacing().item_spacing.x + 6.0;
                    let seg_w = (ui.available_width() - reserve).max(120.0);
                    ui.scope(|ui| {
                    ui.set_max_width(seg_w);
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
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pill(ui, Some(I_DOWNLOAD), "скачать", true, true).on_hover_text("Ctrl+Enter").clicked() {
                            self.add_and_start(&ctx);
                        }
                        if pill(ui, Some(I_PASTE), "вставить", false, true).on_hover_text("вставить ссылку из буфера и сразу скачать").clicked() {
                            self.paste();
                            self.add_and_start(&ctx);
                        }
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(dens(34.0)), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("открыть папку загрузок");
                        let style = pal().lay.buttons;
                        let fill = match style {
                            4 => if resp.hovered() { hover_of(pal().pill) } else { pal().pill },
                            _ if resp.hovered() => pal().surface_hi,
                            2 => Color32::TRANSPARENT,
                            _ => pal().surface,
                        };
                        ui.painter().rect_filled(r, btn_rad(11), fill);
                        if style != 3 && style != 4 {
                            ui.painter().rect_stroke(r, btn_rad(11), Stroke::new(1.0, pal().border), StrokeKind::Inside);
                        }
                        paint_folder(ui.painter(), r.center(), pal().text);
                        if resp.clicked() {
                            let _ = std::fs::create_dir_all(&self.s.out_dir);
                            open_path(Path::new(&self.s.out_dir));
                        }
                    });
                });
                ui.add_space(6.0);
                // кратко о текущих настройках — щелчок открывает настройки
                if spotify {
                    egui::Frame::new().fill(pal().warn_bg).corner_radius(rad(10)).inner_margin(margin(12.0, 8.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(
                            "spotify отдаёт программе только первые 100 треков плейлиста. если треков больше — откройте плейлист                              в браузере и нажмите там кнопку «скачать плейлист» (расширение very good downloader): она передаст весь список.",
                        ).size(11.5).color(pal().warn)).wrap());
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
                    egui::Frame::new().fill(pal().surface).stroke(Stroke::new(1.0, pal().border)).corner_radius(rad(10)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("скопирована ссылка:").size(12.0).color(pal().muted));
                            let w = ui.available_width() - 150.0;
                            ui.allocate_ui(Vec2::new(w.max(60.0), 20.0), |ui| {
                                ui.add(egui::Label::new(RichText::new(&url).size(12.0).color(pal().text)).truncate());
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
                            ui.label(RichText::new("—").color(pal().muted));
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
                    ui.label(RichText::new("·").size(11.5).color(pal().faint));
                    let summary = if music_mode { self.music_summary() } else { self.summary() };
                    let r = ui.add(egui::Label::new(RichText::new(summary).size(11.5).color(pal().faint)).sense(Sense::click()).truncate());
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
                                paint_folder(ui.painter(), r.center(), if is_current { pal().text } else { pal().muted });
                                let w = (ui.available_width() - 90.0).max(60.0);
                                ui.allocate_ui(Vec2::new(w, 20.0), |ui| {
                                    ui.add(egui::Label::new(RichText::new(d.as_str()).size(12.5).color(if is_current { pal().text } else { pal().muted })).truncate());
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if is_current {
                                        ui.label(RichText::new("сейчас").size(11.5).color(pal().ok));
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
                    ui.label(RichText::new("очередь").size(14.0).color(pal().text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.q.jobs.iter().any(|j| !matches!(j.state, State::Queued | State::Running))
                            && link(ui, "убрать готовые").clicked()
                        {
                            self.clear_done();
                        }
                    });
                });
                ui.add_space(4.0);
                if self.q.jobs.is_empty() {
                    egui::Frame::new().stroke(Stroke::new(1.0, pal().outline)).corner_radius(rad(12)).inner_margin(margin(14.0, 14.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("пусто — вставь ссылку выше или нажми «скачать» в браузере").size(12.0).color(pal().faint));
                    });
                }
                let mut actions = Vec::new();
                let n = self.q.jobs.len();
                let mut opts_changed = false;
                for i in 0..n {
                    job_card(ui, &self.q.jobs[i], i == 0, i + 1 == n, &mut actions);
                    let j = &mut self.q.jobs[i];
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
                    ui.label(RichText::new(&self.status).size(11.5).color(pal().muted));
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
                ui.label(heading("настройки", 17.0));
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
                        paint_folder(ui.painter(), r.center(), pal().muted);
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
                    row(ui, "категории и уже скачанное", "«категории» — папки, по которым раскладываются видео, и слова, по которым они выбираются. «разложить» — разложить уже скачанные программой видео из корня папки: сначала покажет план.", |ui| {
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
                        ui.label(RichText::new(if logged { "вход выполнен" } else { "нет входа" }).size(11.5).color(if logged { pal().ok } else { pal().muted }));
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
                        ui.label(RichText::new(if logged { "вход выполнен" } else { "нет входа" }).size(11.5).color(if logged { pal().ok } else { pal().muted }));
                    });
                    // Идёт вход: код крупно — его вводят на странице Яндекса.
                    if let Some((code, url)) = self.ya_code.clone() {
                        egui::Frame::new().fill(pal().surface_hi).corner_radius(rad(10)).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("введите этот код на странице яндекса — латиницей (английская раскладка):").size(12.0).color(pal().muted));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&code).size(26.0).color(pal().text).strong());
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
                            ui.label(RichText::new("после подтверждения программа заметит вход сама.").size(11.0).color(pal().faint));
                        });
                    }
                    divider(ui);
                    row(ui, "ключ song.link (необязательно)", "song.link (odesli) по треку spotify, apple, яндекса или deezer даёт точные ссылки на тот же трек на youtube music, soundcloud и youtube — без поиска по названию. с осени 2026 их api работает только с ключом (запрос — на odesli.co). без ключа программа просто ищет как обычно.", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.s.odesli_key).password(true).hint_text("ключ").desired_width(180.0));
                    });
                    divider(ui);
                    row(ui, "ключ spotify (необязательно)", "плейлисты целиком — только кнопкой расширения «скачать плейлист» на странице spotify: с февраля 2026 api spotify по ключу не отдаёт содержимое чужих плейлистов. ключ помогает лишь с альбомами и треками и работает, только если у владельца приложения spotify есть premium: developer.spotify.com → dashboard → create app; в поле redirect uri впишите https://example.com/callback (адрес не используется); галочка web api → save → settings → скопируйте client id и client secret сюда.", |ui| {
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
                    row(ui, "запускать при входе в windows", "программа тихо стартует в трее, без окна, — кнопки «скачать» в браузере и spotify срабатывают сразу, а о скачанном скажет уведомление. окно — щелчком по значку у часов.", |ui| {
                        if toggle(ui, &mut self.autostart) {
                            if let Ok(exe) = std::env::current_exe() {
                                vgd::appinfo::set_autostart(self.autostart, &exe);
                            }
                        }
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

                if self.group(ui, "оформление") {
                    self.ui_appearance(ui);
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
                    self.ui_update_rows(ui);
                });
                }
                if !self.status.is_empty() && self.side_task {
                    hint(ui, &self.status);
                }
                ui.add_space(14.0);
                ui.label(RichText::new(format!("{APP_NAME} {APP_VERSION} · yt-dlp + ffmpeg · настройки сохраняются сразу")).size(10.5).color(pal().faint));
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
                    ui.label(heading("подписки", 17.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_pill_enabled(ui, "проверить все", !self.s.subscriptions.is_empty()).clicked() {
                            self.check_subs(None);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(
                    "каналы и плейлисты (youtube, soundcloud, youtube music, tiktok…): программа качает только новые видео и треки. \
                     при первой проверке она запоминает, что уже есть, — старое не качается.",
                ).size(11.5).color(pal().muted)).wrap());
                ui.add_space(10.0);
                // добавить
                egui::Frame::new().fill(pal().surface).stroke(Stroke::new(1.0, pal().border)).corner_radius(rad(12)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(glyph(I_LINK).to_string()).family(icons()).size(15.0).color(pal().muted));
                        let w = ui.available_width() - 110.0;
                        ui.add(egui::TextEdit::singleline(&mut self.sub_new).frame(egui::Frame::NONE).desired_width(w).hint_text(RichText::new("ссылка на канал или плейлист").color(pal().faint)));
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
                    ui.label(RichText::new("пока нет подписок").size(12.0).color(pal().faint));
                }
                let mut remove = None;
                let mut check = None;
                for (i, sub) in self.s.subscriptions.iter_mut().enumerate() {
                    egui::Frame::new().fill(pal().surface).corner_radius(rad(12)).inner_margin(Margin { left: 14, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let title = if sub.name.is_empty() { sub.url.clone() } else { sub.name.clone() };
                            let w = ui.available_width() - 70.0;
                            ui.allocate_ui(Vec2::new(w.max(60.0), 20.0), |ui| {
                                ui.add(egui::Label::new(RichText::new(title).size(13.5).color(pal().text)).truncate()).on_hover_text(&sub.url);
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
                            ui.label(RichText::new(ago(sub.last_check)).size(11.5).color(pal().faint));
                            ui.add_space(10.0);
                            toggle(ui, &mut sub.audio_only);
                            ui.label(RichText::new("только звук").size(11.5).color(pal().muted));
                            ui.add_space(10.0);
                            ui.label(RichText::new("не старше").size(11.5).color(pal().muted));
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
            ui.label(RichText::new("chrome, edge, яндекс.браузер и firefox не найдены").size(12.0).color(pal().muted));
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
                Status::On => ("кнопки на месте", pal().ok),
                Status::Off if self.s.spotify_app => ("кнопки пропали (spotify обновился) — вернутся сами, когда spotify закрыт", pal().warn),
                Status::Off => ("не установлено", pal().muted),
                Status::Store => ("spotify из microsoft store не поддерживается", pal().err),
                Status::NoSpotify => ("spotify для windows не найден", pal().muted),
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
            egui::Frame::new().fill(pal().ok_bg).corner_radius(rad(12)).inner_margin(margin(14.0, 10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("поставьте расширение — на сайтах появятся кнопки ↓").size(13.0).color(pal().text));
                ui.add(egui::Label::new(RichText::new("выберите браузер: программа откроет его и подскажет, что нажать. это минута.").size(12.0).color(pal().muted)).wrap());
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
            // Подпись короткая: длинный список сайтов в узком окне наезжал на «подключено».
            row(ui, "кнопки ↓ на сайтах", "youtube, tiktok, spotify, soundcloud, яндекс музыка, apple music и vk — кнопка ↓ появляется прямо на странице. на любом другом сайте — значок расширения или пункт в меню правой кнопки.", |ui| {
                ui.label(RichText::new(status).size(12.0).color(if seen.is_some() { pal().ok } else { pal().muted }));
            });
            divider(ui);
            row(ui, "установить в браузер", "chrome, edge и яндекс.браузер не дают программам ставить расширения сами — программа подготовит папку, скопирует её путь и откроет страницу расширений. в firefox расширение ставится одной кнопкой «добавить».", |ui| {
                self.browser_buttons(ui);
            });
        });
        if let Some(name) = self.ext_steps {
            ui.add_space(6.0);
            egui::Frame::new().fill(pal().ok_bg).corner_radius(rad(12)).inner_margin(margin(14.0, 10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let firefox = name == "Firefox";
                ui.label(RichText::new(if firefox { "осталось нажать «добавить» в firefox:".to_string() } else { format!("осталось два щелчка в «{name}»:") }).size(12.5).color(pal().text));
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
                    ui.add(egui::Label::new(RichText::new(step).size(12.0).color(pal().muted)).wrap());
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
                ui.label(heading("журнал", 17.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if small_pill_enabled(ui, "очистить", !self.log.is_empty()).clicked() {
                        self.log.clear();
                    }
                    // С поиском копируется то, что найдено.
                    if small_pill_enabled(ui, "копировать", !self.log.is_empty()).clicked() {
                        let q = self.log_find.trim().to_lowercase();
                        let lines: Vec<&str> = self.log.iter().filter(|l| q.is_empty() || l.to_lowercase().contains(&q)).map(String::as_str).collect();
                        ui.ctx().copy_text(lines.join("\n"));
                    }
                });
            });
            ui.add_space(8.0);
            let query = self.log_find.trim().to_lowercase();
            let found = if query.is_empty() { self.log.len() } else { self.log.iter().filter(|l| l.to_lowercase().contains(&query)).count() };
            egui::Frame::new().fill(pal().surface).corner_radius(rad(10)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.log_find.is_empty() {
                            if link(ui, "сбросить").clicked() {
                                self.log_find.clear();
                            }
                            ui.label(RichText::new(format!("найдено: {found}")).size(12.0).color(if found > 0 { pal().muted } else { pal().warn }));
                        }
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.log_find)
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width())
                                .hint_text(RichText::new("поиск в журнале — ctrl+f").color(pal().faint)),
                        );
                        if std::mem::take(&mut self.log_find_focus) {
                            field.request_focus();
                        }
                        // Esc в поле поиска — сбросить его.
                        if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.log_find.clear();
                        }
                    });
                });
            });
            ui.add_space(8.0);
            egui::Frame::new().fill(pal().surface).corner_radius(rad(12)).inner_margin(12).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.log.is_empty() {
                            ui.label(RichText::new("пока пусто").color(pal().faint));
                        } else if found == 0 {
                            ui.label(RichText::new("ничего не найдено").color(pal().faint));
                        }
                        for l in self.log.iter().filter(|l| query.is_empty() || l.to_lowercase().contains(&query)) {
                            let color = if l.starts_with("Ошибка") {
                                pal().err
                            } else if l.starts_with("Внимание") || l.starts_with("Предупреждение") {
                                pal().warn
                            } else if l.starts_with("Скачано") || l.starts_with("Готово") {
                                pal().text
                            } else {
                                pal().muted
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
            ui.label(heading(&format!("разложить {} файл(ов)?", plan.len()), 18.0));
            ui.add_space(4.0);
            ui.label(RichText::new("сначала посмотрите, куда что переедет").size(12.0).color(pal().muted));
            ui.add_space(10.0);
            egui::Frame::new().fill(pal().bg).corner_radius(rad(10)).inner_margin(10).show(ui, |ui| {
                egui::ScrollArea::both().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (src, dst) in plan {
                        let rel = dst
                            .parent()
                            .and_then(|p| p.strip_prefix(&self.sort_root).ok())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(rel).size(12.0).color(pal().text));
                            ui.label(RichText::new(format!("←  {name}")).size(12.0).color(pal().muted));
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
            ui.label(heading("категории", 18.0));
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(
                "категория — это папка в папке загрузок. видео попадает в неё, если в названии, тегах, нике канала или плейлисте есть её слова. \
                 при равенстве выигрывает категория выше. ничего не подошло — рубрика youtube (игры, музыка…) или «разное».",
            ).size(12.0).color(pal().muted)).wrap());
            ui.add_space(10.0);
            egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                for (i, (_, name, words)) in rows.iter_mut().enumerate() {
                    egui::Frame::new().fill(pal().surface).corner_radius(rad(10)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
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
            ui.label(heading("идёт загрузка", 18.0));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new("прервать её и выйти? уже скачанное сохранится, остальное докачается при следующем запуске.")
                        .size(12.5)
                        .color(pal().muted),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, None, "выйти", true, true).clicked() {
                    self.q.stop_all();
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
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../app.png"))
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
                self.q.stop_all();
                self.save_settings();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Запуск в фоне: eframe показывает окно после первого кадра — прячем обратно.
        if self.hidden && self.hide_frames < 4 {
            self.hide_frames += 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.request_repaint();
        }
        // Окно впервые показывается после запуска в фоне — с края экрана в центр.
        if self.offscreen && !self.hidden {
            self.offscreen = false;
            let (monitor, rect) = ctx.input(|i| (i.viewport().monitor_size, i.viewport().outer_rect));
            if let (Some(m), Some(r)) = (monitor, rect) {
                let pos = egui::pos2(((m.x - r.width()) / 2.0).max(0.0), ((m.y - r.height()) / 2.0).max(0.0));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(120.0, 80.0)));
            }
        }
        // Подписки — и при запуске в фоне, когда окно ещё не показано.
        if !self.subs_started {
            self.subs_started = true;
            if self.s.subs_on_start && !self.s.subscriptions.is_empty() {
                self.check_subs(None);
            }
        }
        // Обновления — и в фоне: найдётся новая версия, пока окно в трее, — скажет уведомление.
        self.update_start();
        self.update_tick();
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
        self.handle_events(&ctx);
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
                        if !first && supported_link(&t) && !self.q.jobs.iter().any(|j| j.url == t) && self.urls.trim() != t {
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
        // Ctrl+F с любой страницы — журнал с курсором в поиске.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
            self.page = Page::Log;
            self.log_find_focus = true;
        }

        // Навигация: слева (как раньше) или полосой сверху/снизу — по теме.
        let nav_frame = egui::Frame::new().fill(pal().sidebar).stroke(Stroke::new(1.0, pal().sidebar_line));
        match pal().lay.nav {
            1 => {
                egui::Panel::top("nav_top").exact_size(dens(50.0)).resizable(false).frame(nav_frame).show(ui, |ui| self.ui_navbar(ui));
            }
            2 => {
                egui::Panel::bottom("nav_bottom").exact_size(dens(54.0)).resizable(false).frame(nav_frame).show(ui, |ui| self.ui_navbar(ui));
            }
            _ => {
                egui::Panel::left("nav").exact_size(pal().lay.nav_width).resizable(false).frame(nav_frame).show(ui, |ui| self.ui_sidebar(ui));
            }
        }
        egui::CentralPanel::default_margins().frame(egui::Frame::new().fill(pal().bg)).show(ui, |ui| {
            paint_background(ui);
            match self.page {
                Page::Download => self.ui_download(ui),
                Page::Settings => self.ui_settings(ui),
                Page::Subs => self.ui_subs(ui),
                Page::Log => self.ui_log(ui),
            }
        });

        self.ui_sort_plan(&ctx);
        self.ui_topics(&ctx);
        self.ui_confirm_exit(&ctx);
        self.ui_update_ask(&ctx);
        self.watch_theme_file(&ctx);
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

/// Колонка по центру шириной до layout.column из темы.
fn centered(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let avail = ui.available_width();
    let w = (avail - 40.0).min(pal().lay.column);
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
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(pal().muted)).wrap());
}

fn text_size(ui: &egui::Ui, text: &str, font: &FontId) -> Vec2 {
    ui.painter().layout_no_wrap(text.to_string(), font.clone(), pal().text).size()
}

/// Сегментный переключатель: несколько вариантов в одной плашке, выбранный — светлый.
fn segmented<T: PartialEq + Clone>(ui: &mut egui::Ui, value: &mut T, opts: &[(T, String)]) -> bool {
    let mut changed = false;
    let font = FontId::monospace(13.0);
    let total: f32 = opts.iter().map(|(_, l)| (text_size(ui, l, &font).x + 24.0 * pal().lay.padding).max(48.0) + 2.0).sum::<f32>() + 6.0;
    let width = total.min(ui.available_width());
    let tonal = pal().lay.buttons == 4;
    let frame = if tonal {
        egui::Frame::new().stroke(Stroke::new(1.0, pal().border))
    } else {
        egui::Frame::new().fill(pal().surface)
    };
    frame.corner_radius(btn_rad(11)).inner_margin(3).show(ui, |ui| {
        ui.set_max_width(width - 6.0);
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        ui.horizontal_wrapped(|ui| {
            // В строках настроек кнопки стоят справа (раскладка справа налево) — порядок сохраняем.
            let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
            let items: Vec<&(T, String)> = if rtl { opts.iter().rev().collect() } else { opts.iter().collect() };
            for (v, label) in items {
                let size = text_size(ui, label, &font) + Vec2::new(24.0 * pal().lay.padding, 0.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(size.x.max(48.0), dens(30.0)), Sense::click());
                let selected = v == value;
                let fill = if selected && tonal {
                    pal().pill
                } else if selected {
                    pal().accent
                } else if resp.hovered() {
                    pal().surface_hi
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, btn_rad(8), fill);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    font.clone(),
                    if selected && !tonal { pal().on_accent } else { pal().text },
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
    let p = pal();
    let f = egui::Frame::new().corner_radius(rad(12)).inner_margin(margin(12.0, 6.0)).shadow(card_shadow());
    let f = match p.lay.cards {
        1 => f.fill(p.surface.gamma_multiply(0.55)).stroke(Stroke::new(1.0, p.border)),
        2 => f,
        _ => f.fill(p.surface),
    };
    f.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, pal().divider);
}

/// Строка: подпись слева (подсказка при наведении), элементы управления справа.
fn row(ui: &mut egui::Ui, label: &str, tip: &str, add: impl FnOnce(&mut egui::Ui)) {
    let open_id = egui::Id::unique(("tip", label));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), dens(38.0)), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(dens(38.0));
        ui.label(RichText::new(label).size(13.0).color(pal().text));
        if !tip.is_empty() {
            // Кружок «?»: щелчок показывает/скрывает пояснение под строкой.
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let hot = resp.hovered() || open;
            ui.painter().circle_stroke(rect.center(), 7.5, Stroke::new(1.0, if hot { pal().text } else { pal().faint }));
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "?", FontId::monospace(10.5), if hot { pal().text } else { pal().muted });
            if resp.clicked() {
                open = !open;
                ui.data_mut(|d| d.insert_temp(open_id, open));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
    if open && !tip.is_empty() {
        ui.add(egui::Label::new(RichText::new(tip).size(11.5).color(pal().muted)).wrap());
        ui.add_space(6.0);
    }
}

/// Переключатель-тумблер.
fn toggle(ui: &mut egui::Ui, value: &mut bool) -> bool {
    if pal().lay.m3_switch {
        return toggle_m3(ui, value);
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui.ctx().animate_bool(resp.id, *value);
    let p = ui.painter();
    p.rect_filled(rect, rad(10), pal().toggle_off.lerp_to_gamma(pal().accent, t));
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    p.circle_filled(egui::pos2(x, rect.center().y), 7.0, pal().knob_off.lerp_to_gamma(pal().knob_on, t));
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// Тумблер в духе Material 3 Switch: выключен — рамка и маленький кружок, включён — залит акцентом, кружок больше.
fn toggle_m3(ui: &mut egui::Ui, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(44.0, 26.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui.ctx().animate_bool(resp.id, *value);
    let c = pal();
    let p = ui.painter();
    let r = rect.height() / 2.0;
    p.rect_filled(rect, r as u8, c.toggle_off.lerp_to_gamma(c.accent, t));
    if t < 1.0 {
        p.rect_stroke(rect, r as u8, Stroke::new(2.0, c.knob_off.gamma_multiply(1.0 - t)), StrokeKind::Inside);
    }
    let x = egui::lerp(rect.left() + r..=rect.right() - r, t);
    let knob = egui::lerp(6.0..=9.5, t) + if resp.hovered() { 1.0 } else { 0.0 };
    p.circle_filled(egui::pos2(x, rect.center().y), knob, c.knob_off.lerp_to_gamma(c.knob_on, t));
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
    let size = text_size(ui, text, &font) + Vec2::new(pad(22.0), 0.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(size.x, dens(28.0)), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let p = ui.painter();
    let style = pal().lay.buttons;
    let fill = if hovered { pal().surface_hi } else if style == 2 || style == 4 { Color32::TRANSPARENT } else { pal().pill };
    p.rect_filled(rect, btn_rad(9), fill);
    if style != 3 {
        p.rect_stroke(rect, btn_rad(9), Stroke::new(1.0, pal().border), StrokeKind::Inside);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, if enabled { pal().text } else { pal().faint });
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Ширина кнопки pill() со значком.
fn pill_width(ui: &egui::Ui, text: &str) -> f32 {
    text_size(ui, text, &FontId::monospace(13.5)).x + 22.0 + pad(28.0)
}

/// Кнопка-«пилюля»: основная — светлая, остальные — тёмные.
fn pill(ui: &mut egui::Ui, icon: Option<char>, text: &str, primary: bool, enabled: bool) -> egui::Response {
    let font = FontId::monospace(13.5);
    let ifont = FontId::new(14.0, icons());
    let tw = text_size(ui, text, &font).x;
    let iw = if icon.is_some() { 22.0 } else { 0.0 };
    let size = Vec2::new(tw + iw + pad(28.0), dens(34.0));
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let c = pal();
    let style = c.lay.buttons;
    let (fill, fg) = match (primary, enabled) {
        (true, true) if style == 2 => (if hovered { c.accent.gamma_multiply(0.18) } else { Color32::TRANSPARENT }, c.accent),
        (true, true) => (if hovered { c.accent_hover } else { c.accent }, c.on_accent),
        (false, true) if style == 2 => (if hovered { c.surface_hi } else { Color32::TRANSPARENT }, c.text),
        (false, true) if style == 4 => (if hovered { hover_of(c.pill) } else { c.pill }, c.text),
        (false, true) => (if hovered { c.surface_hi } else { c.surface }, c.text),
        (_, false) => (if style == 2 { Color32::TRANSPARENT } else { c.surface }, c.faint),
    };
    let stroke = match (style, primary, enabled) {
        (3, _, _) | (4, _, true) => Stroke::NONE,
        (2, true, true) => Stroke::new(1.5, if hovered { c.accent_hover } else { c.accent }),
        (2, _, _) => Stroke::new(1.0, c.border),
        (_, true, _) => Stroke::NONE,
        _ => Stroke::new(1.0, c.border),
    };
    let p = ui.painter();
    p.rect_filled(rect, btn_rad(11), fill);
    if stroke != Stroke::NONE {
        p.rect_stroke(rect, btn_rad(11), stroke, StrokeKind::Inside);
    }
    let mut x = rect.left() + pad(14.0);
    if let Some(ic) = icon {
        p.text(egui::pos2(x, rect.center().y + 1.0), Align2::LEFT_CENTER, glyph(ic), ifont, fg);
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
    let r = ui.add(egui::Label::new(RichText::new(text).size(12.0).color(pal().muted)).sense(Sense::click()));
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() {
        let y = r.rect.bottom();
        ui.painter().line_segment([egui::pos2(r.rect.left(), y), egui::pos2(r.rect.right(), y)], Stroke::new(1.0, pal().muted));
    }
    r
}

/// Значок без рамки (для карточек очереди).
fn icon_button(ui: &mut egui::Ui, icon: char, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(rect, rad(8), pal().surface_hi);
    }
    let color = if resp.hovered() { pal().text } else { pal().muted };
    if icon == I_FOLDER {
        paint_folder(ui.painter(), rect.center(), color);
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph(icon), FontId::new(12.0, icons()), color);
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
    if pal().lay.nav_pill {
        // Navigation Rail: «таблетка» вокруг значка, подпись под ней
        let ind = egui::Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 18.0), Vec2::new(54.0, 30.0));
        if selected {
            p.rect_filled(ind, 15, pal().pill);
        } else if resp.hovered() {
            p.rect_filled(ind, 15, pal().surface_hi);
        }
    } else if selected {
        p.rect_filled(rect, rad(10), pal().surface_hi);
    } else if resp.hovered() {
        p.rect_filled(rect, rad(10), pal().surface);
    }
    let fg = if selected || resp.hovered() { pal().text } else { pal().muted };
    p.text(rect.center_top() + Vec2::new(0.0, 19.0), Align2::CENTER_CENTER, glyph(icon), FontId::new(17.0, icons()), fg);
    p.text(rect.center_bottom() - Vec2::new(0.0, 13.0), Align2::CENTER_CENTER, label, FontId::monospace(11.0), fg);
    if let Some(n) = badge {
        let c = rect.right_top() + Vec2::new(-14.0, 12.0);
        p.circle_filled(c, 8.0, pal().accent);
        p.text(c, Align2::CENTER_CENTER, n.min(99).to_string(), FontId::monospace(10.0), pal().on_accent);
    }
    resp
}

fn nav_row_width(ui: &egui::Ui, label: &str, labels: bool, font: &FontId) -> f32 {
    if labels { text_size(ui, label, font).x + 46.0 } else { 44.0 }
}

/// Пункт навигации в строку: значок и подпись рядом (или только значок с подсказкой).
fn nav_row(ui: &mut egui::Ui, icon: char, label: &str, selected: bool, badge: Option<usize>, width: f32, labels: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, dens(38.0)), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let resp = if labels { resp } else { resp.on_hover_text(label) };
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, btn_rad(10), pal().surface_hi);
    } else if resp.hovered() {
        p.rect_filled(rect, btn_rad(10), pal().surface);
    }
    let fg = if selected || resp.hovered() { pal().text } else { pal().muted };
    let ifont = FontId::new(16.0, icons());
    if labels {
        p.text(egui::pos2(rect.left() + 20.0, rect.center().y + 1.0), Align2::CENTER_CENTER, glyph(icon), ifont, fg);
        p.text(egui::pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::monospace(12.0), fg);
    } else {
        p.text(rect.center() + Vec2::new(0.0, 1.0), Align2::CENTER_CENTER, glyph(icon), ifont, fg);
    }
    if let Some(n) = badge {
        let c = if labels { egui::pos2(rect.right() - 14.0, rect.center().y) } else { rect.right_top() + Vec2::new(-8.0, 8.0) };
        p.circle_filled(c, 8.0, pal().accent);
        p.text(c, Align2::CENTER_CENTER, n.min(99).to_string(), FontId::monospace(10.0), pal().on_accent);
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
                    ui.label(RichText::new("вместо оригинала").size(11.5).color(pal().muted));
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
                ui.add(egui::Label::new(RichText::new(short_dir(&cur.out_dir)).size(12.0).color(pal().muted)).truncate()).on_hover_text(&cur.out_dir);
            });
        });
        if next {
            divider(ui);
            ui.add_space(4.0);
            ui.label(RichText::new("эти настройки — только для ссылок, добавленных сейчас; общие не меняются.").size(11.0).color(pal().faint));
            ui.add_space(4.0);
        }
    });
    changed
}

/// Карточка одной ссылки в очереди. `first` / `last` — можно ли сдвинуть вверх / вниз.
fn job_card(ui: &mut egui::Ui, j: &Job, first: bool, last: bool, actions: &mut Vec<Action>) {
    let resp = egui::Frame::new()
        .fill(pal().surface)
        .corner_radius(rad(12))
        .inner_margin(Margin { left: 14, right: 8, top: 10, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (status, color) = match j.state {
                    State::Done => (stage_name("done").to_string(), pal().ok),
                    State::Error => (stage_name("error").to_string(), pal().err),
                    State::Cancelled => (stage_name("cancel").to_string(), pal().faint),
                    State::Queued => (stage_name("queued").to_string(), pal().muted),
                    State::Paused => (stage_name("paused").to_string(), pal().warn),
                    State::Running => {
                        let mut s = stage_name(&j.stage).to_string();
                        if !j.item.is_empty() {
                            s = format!("{} · {s}", j.item);
                        }
                        (s, if j.stage == "wait" { pal().warn } else { pal().text })
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
                    let tcolor = if j.state == State::Cancelled { pal().faint } else { pal().text };
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
                ui.painter().rect_filled(rect, 2, pal().track);
                let frac = (j.percent / 100.0).clamp(0.0, 1.0);
                if frac > 0.0 {
                    let mut fill = rect;
                    fill.set_width((rect.width() * frac).max(4.0));
                    ui.painter().rect_filled(fill, 2, if j.stage == "wait" { pal().warn } else { pal().accent });
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
                ui.add(egui::Label::new(RichText::new(parts.join("  ·  ")).size(11.5).color(pal().muted)).truncate());
            } else if !j.error.is_empty() || !j.message.is_empty() {
                ui.add_space(4.0);
                let (t, c) = if j.error.is_empty() { (j.message.as_str(), pal().muted) } else { (j.error.as_str(), pal().err) };
                ui.add(egui::Label::new(RichText::new(t).size(11.5).color(c)).wrap());
            }
            if !j.notice.is_empty() {
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&j.notice).size(11.5).color(pal().warn)).wrap());
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

thread_local! {
    /// для каких шрифта/значков/размера уже собраны шрифты
    static FONT_KEY: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// картинка фона темы (layout.background_image) и её имя
    static BG_IMG: std::cell::RefCell<(String, Option<egui::TextureHandle>)> = const { std::cell::RefCell::new((String::new(), None)) };
}

/// Применить тему: цвета, скругление, раскладку; шрифты и картинка фона — только если они сменились.
fn apply_theme(ctx: &egui::Context, t: &Theme) {
    PAL.with(|p| p.set(Pal::of(t)));
    let key = format!("{}|{}|{}|{}", t.font, t.icons, t.layout.font_scale, t.layout.bold_headings);
    if FONT_KEY.with(|k| *k.borrow() != key) {
        setup_fonts(ctx, t);
        FONT_KEY.with(|k| *k.borrow_mut() = key);
    }
    let img = t.layout.background_image.clone();
    if BG_IMG.with(|b| b.borrow().0 != img) {
        let tex = (!img.is_empty())
            .then(|| std::fs::read(theme::dir().join(&img)).ok())
            .flatten()
            .and_then(|bytes| eframe::icon_data::from_png_bytes(&bytes).ok())
            .map(|ic| {
                let ci = egui::ColorImage::from_rgba_unmultiplied([ic.width as usize, ic.height as usize], &ic.rgba);
                ctx.load_texture("theme_bg", ci, egui::TextureOptions::LINEAR)
            });
        BG_IMG.with(|b| *b.borrow_mut() = (img, tex));
    }
    setup_style(ctx, t.is_light());
}

fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn setup_fonts(ctx: &egui::Context, t: &Theme) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dir = PathBuf::from(windir).join("Fonts");
    let scale = t.layout.font_scale;
    let data = |bytes: Vec<u8>, s: f32| {
        let d = FontData::from_owned(bytes);
        Arc::new(if s == 1.0 { d } else { d.tweak(egui::epaint::text::FontTweak { scale: s, ..Default::default() }) })
    };
    // Моноширинный шрифт для всего интерфейса: Cascadia Mono (Windows 11 / Terminal), иначе Consolas.
    // В теме со шрифтом «sans» — Segoe UI.
    let files: &[&str] = match t.font.as_str() {
        "sans" => &["segoeui.ttf", "CascadiaMono.ttf", "consola.ttf"],
        "roboto" => &["Roboto-Regular.ttf", "segoeui.ttf", "CascadiaMono.ttf", "consola.ttf"],
        _ => &["CascadiaMono.ttf", "consola.ttf"],
    };
    let mono = files.iter().find_map(|f| std::fs::read(dir.join(f)).ok().map(|b| (*f, b)));
    if let Some((_, bytes)) = &mono {
        fonts.font_data.insert("mono".into(), data(bytes.clone(), scale));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().insert(0, "mono".into());
        }
    }
    // Жирный для заголовков: Segoe UI Bold, Cascadia Mono с толщиной 700 (шрифт переменный) или Consolas Bold.
    let mut bold_fam = Vec::new();
    if t.layout.bold_headings {
        let b = match &mono {
            // у Roboto заголовки — Medium (как title в шкале Material 3), у Segoe UI — Semibold/Bold
            Some(("Roboto-Regular.ttf", _)) => std::fs::read(dir.join("Roboto-Medium.ttf")).ok().map(|b| data(b, scale)),
            Some(("segoeui.ttf", _)) => {
                let f = if t.font == "roboto" { "seguisb.ttf" } else { "segoeuib.ttf" };
                std::fs::read(dir.join(f)).ok().map(|b| data(b, scale))
            }
            Some(("CascadiaMono.ttf", bytes)) => Some(Arc::new(FontData::from_owned(bytes.clone()).tweak(
                egui::epaint::text::FontTweak {
                    scale,
                    coords: egui::epaint::text::VariationCoords::new([("wght", 700.0)]),
                    ..Default::default()
                },
            ))),
            _ => std::fs::read(dir.join("consolab.ttf")).ok().map(|b| data(b, scale)),
        };
        if let Some(b) = b {
            fonts.font_data.insert("bold".into(), b);
            bold_fam.push("bold".to_string());
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
            if key == "sym" && t.icons == "symbols" {
                // для значков — крупнее, чтобы не терялись рядом с текстом
                fonts.font_data.insert("sym_icons".into(), data(bytes.clone(), 1.3));
            }
            fonts.font_data.insert(key.into(), data(bytes, scale));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(key.into());
            }
        }
    }
    let text_fam = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    bold_fam.extend(text_fam.iter().cloned());
    fonts.families.insert(bold(), bold_fam);
    if t.icons == "symbols" {
        // Значки-символы: Segoe UI Symbol (крупнее), затем основной шрифт.
        let mut fam = text_fam.clone();
        if fonts.font_data.contains_key("sym_icons") {
            fam.insert(0, "sym_icons".into());
        }
        fonts.families.insert(icons(), fam);
    } else if let Ok(bytes) = std::fs::read(dir.join("segmdl2.ttf")) {
        fonts.font_data.insert("icons".into(), data(bytes, scale));
        fonts.families.insert(icons(), vec!["icons".into()]);
    } else {
        // Без значков: пусть хотя бы не будет ошибки шрифта.
        fonts.families.insert(icons(), text_fam);
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context, light: bool) {
    let base = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    // Обе темы egui (светлая/тёмная Windows) — одинаковые, чтобы окно не зависело от системы.
    ctx.all_styles_mut(|s| {
        s.visuals = base.clone();

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
        v.panel_fill = pal().bg;
        v.window_fill = pal().window;
        v.window_stroke = Stroke::new(1.0, pal().border);
        v.window_corner_radius = CornerRadius::same(rad(16));
        v.menu_corner_radius = CornerRadius::same(rad(10));
        v.extreme_bg_color = pal().field;
        v.faint_bg_color = pal().surface;
        v.override_text_color = None;
        v.selection.bg_fill = pal().selection;
        v.selection.stroke = Stroke::new(1.0, pal().text);
        v.hyperlink_color = pal().text;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(rad(10));
            w.bg_stroke = Stroke::new(1.0, pal().border);
        }
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal().text);
        v.widgets.noninteractive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.bg_fill = pal().surface;
        v.widgets.inactive.weak_bg_fill = pal().surface;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, pal().text);
        v.widgets.hovered.bg_fill = pal().surface_hi;
        v.widgets.hovered.weak_bg_fill = pal().surface_hi;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, pal().text_hover);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal().muted);
        v.widgets.active.bg_fill = pal().accent;
        v.widgets.active.weak_bg_fill = pal().accent;
        v.widgets.active.fg_stroke = Stroke::new(1.0, pal().on_accent);
        v.text_cursor.stroke = Stroke::new(2.0, pal().text);
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
    vgd::vk_web::install();
    // При входе в Windows: вернуть кнопки в Spotify, если он обновился, — без окна.
    if std::env::args().any(|a| a == "--spotify-repair") {
        if let Some(Err(e)) = vgd::spotify_app::repair(&|_| {}) {
            eprintln!("{e}");
        }
        return Ok(());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let urls = ipc::launch_urls(args.iter().cloned());
    // `vgd://show` — щелчок по уведомлению «доступна новая версия»: показать окно.
    let show_req = args.iter().any(|a| a.trim().trim_end_matches('/').eq_ignore_ascii_case("vgd://show"));
    // Запуск в фоне: при входе в Windows (`--background`) или из расширения/Spotify (`vgd://…`) — без окна.
    let background = !show_req && args.iter().any(|a| a == "--background" || a.to_ascii_lowercase().starts_with("vgd:"));
    // Одна копия программы. Вторая отдаёт ссылки первой (или просит показать окно) и выходит.
    // Первая могла только что стартовать и ещё не занять порт, или как раз закрываться — пробуем ~5 с.
    if !ipc::single_instance() {
        let mut primary = false;
        for _ in 0..50 {
            if ipc::forward(&urls, !background) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if ipc::single_instance() {
                primary = true;
                break;
            }
        }
        if !primary {
            return Ok(()); // первая копия есть, но не отвечает — вторую не открываем
        }
    }
    // Порт — сразу, до окна: расширение и Spotify получают ответ через доли секунды после запуска,
    // а ссылки ждут в канале, пока окно создаётся.
    let (inbox_tx, inbox) = channel();
    {
        let tx = Mutex::new(inbox_tx.clone());
        ipc::serve(move |m| {
            let _ = tx.lock().map(|t| t.send(m));
        });
    }
    if !urls.is_empty() {
        let _ = inbox_tx.send(ipc::Incoming::Add { urls, ticket: None });
    }
    drop(inbox_tx);
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../app.png")).ok();
    let saved = Settings::load();
    let size = if saved.win_w >= 560.0 && saved.win_h >= 420.0 { [saved.win_w, saved.win_h] } else { [780.0, 600.0] };
    let mut vp = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size(size)
        .with_min_inner_size([560.0, 420.0])
        .with_visible(!background);
    if background {
        // eframe показывает окно после первого кадра; за краем экрана этого не видно — потом прячем.
        vp = vp.with_position([-32000.0, -32000.0]);
    }
    if let Some(icon) = icon {
        vp = vp.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(App::new(cc, inbox, background)))))
}

trait IfEmptyStr {
    fn if_empty(&self, other: &str) -> String;
}

impl IfEmptyStr for str {
    fn if_empty(&self, other: &str) -> String {
        if self.is_empty() { other.to_string() } else { self.to_string() }
    }
}
