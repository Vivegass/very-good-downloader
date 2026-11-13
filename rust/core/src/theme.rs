//! Темы оформления окна: цвета, скругление углов, шрифт и набор значков.
//!
//! Встроенные темы зашиты в программу, свои лежат файлами JSON в
//! `%APPDATA%\VeryGoodDownloader\themes\` — одна тема = один файл, им можно делиться.
//! В файле можно указать не все поля: недостающие берутся из тёмной темы.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

use crate::appinfo::data_dir;


pub mod m3;
use m3::M3;

/// Цвет «#rrggbb».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub const fn hex(v: u32) -> Self {
        Rgb([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    }
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        let h: String = match h.len() {
            3 => h.chars().flat_map(|c| [c, c]).collect(),
            6 => h.to_string(),
            _ => return None,
        };
        u32::from_str_radix(&h, 16).ok().map(Rgb::hex)
    }
    /// Яркость 0..1 — чтобы понять, светлая тема или тёмная.
    pub fn luma(self) -> f32 {
        let [r, g, b] = self.0;
        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("цвет «{s}» — нужно #rrggbb")))
    }
}

/// Шрифт интерфейса.
pub const FONTS: &[(&str, &str)] = &[("mono", "моноширинный"), ("sans", "segoe ui"), ("roboto", "roboto")];
/// Набор значков.
pub const ICON_SETS: &[(&str, &str)] = &[("line", "тонкие"), ("symbols", "символы")];

/// Раскладка и форма — задаются только в файле темы (в окне не редактируются).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// навигация: "left" / "top" / "bottom"
    pub nav: String,
    /// подписи под значками навигации (false — только значки)
    pub nav_labels: bool,
    /// ширина левой навигации; от 140 — значок и подпись в строку
    pub nav_width: f32,
    /// в навигации: "logo" — значок программы, "name" — значок и название, "none" — ничего
    pub header: String,
    /// ширина колонки с содержимым
    pub column: f32,
    /// карточки: "fill" — заливка, "outline" — рамка, "none" — без фона
    pub cards: String,
    /// под карточками: "none" / "shadow" — тень / "glow" — свечение цвета акцента
    pub shadow: String,
    /// плотность: высота строк и кнопок (0.8 — компактно, 1.3 — просторно)
    pub density: f32,
    /// внутренние отступы (ещё растут вместе со скруглением)
    pub padding: f32,
    /// кнопки: "pill" — скруглённые, "rect" — прямоугольные, "outline" — контур, "flat" — плоские,
    /// "tonal" — как в Material 3: главная залита акцентом, обычные — тональные (цвет pill), маленькие — контурные
    pub buttons: String,
    /// скругление кнопок в точках (100 — «таблетка» целиком); -1 — по radius темы
    pub button_radius: f32,
    /// скругление поля для ссылок в точках; -1 — по radius темы
    pub field_radius: f32,
    /// выбранный пункт навигации: "block" — подсветка всего пункта, "pill" — «таблетка» вокруг значка (Navigation Rail)
    pub nav_indicator: String,
    /// тумблеры: "classic" — как в тёмной, "m3" — крупные, с рамкой в выключенном виде (Material 3 Switch)
    pub switch: String,
    /// фон окна — градиент сверху вниз из 2–3 цветов (пусто — сплошной цвет bg)
    pub background: Vec<Rgb>,
    /// картинка PNG из папки тем поверх фона ("" — без картинки)
    pub background_image: String,
    /// непрозрачность картинки 0..1
    pub image_opacity: f32,
    /// размер шрифта (1 — обычный)
    pub font_scale: f32,
    /// жирные заголовки страниц и разделов
    pub bold_headings: bool,
    /// подписи разделов настроек: "lower" — как есть, "upper" — ПРОПИСНЫМИ
    pub caps: String,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            nav: "left".into(),
            nav_labels: true,
            nav_width: 76.0,
            header: "logo".into(),
            column: 640.0,
            cards: "fill".into(),
            shadow: "none".into(),
            density: 1.0,
            padding: 1.0,
            buttons: "pill".into(),
            button_radius: -1.0,
            field_radius: -1.0,
            nav_indicator: "block".into(),
            switch: "classic".into(),
            background: Vec::new(),
            background_image: String::new(),
            image_opacity: 0.25,
            font_scale: 1.0,
            bold_headings: false,
            caps: "lower".into(),
        }
    }
}

fn one_of(v: &mut String, allowed: &[&str]) {
    if !allowed.contains(&v.as_str()) {
        *v = allowed[0].to_string();
    }
}

fn clamp(v: &mut f32, lo: f32, hi: f32, def: f32) {
    *v = if v.is_finite() { v.clamp(lo, hi) } else { def };
}

impl Layout {
    pub fn sanitize(&mut self) {
        one_of(&mut self.nav, &["left", "top", "bottom"]);
        one_of(&mut self.header, &["logo", "name", "none"]);
        one_of(&mut self.cards, &["fill", "outline", "none"]);
        one_of(&mut self.shadow, &["none", "shadow", "glow"]);
        one_of(&mut self.buttons, &["pill", "rect", "outline", "flat", "tonal"]);
        one_of(&mut self.nav_indicator, &["block", "pill"]);
        one_of(&mut self.switch, &["classic", "m3"]);
        clamp(&mut self.button_radius, -1.0, 100.0, -1.0);
        clamp(&mut self.field_radius, -1.0, 100.0, -1.0);
        one_of(&mut self.caps, &["lower", "upper"]);
        clamp(&mut self.nav_width, 60.0, 260.0, 76.0);
        clamp(&mut self.column, 420.0, 1200.0, 640.0);
        clamp(&mut self.density, 0.7, 1.6, 1.0);
        clamp(&mut self.padding, 0.5, 2.5, 1.0);
        clamp(&mut self.image_opacity, 0.0, 1.0, 0.25);
        clamp(&mut self.font_scale, 0.8, 1.4, 1.0);
        self.background.truncate(3);
        // картинка — только имя файла в папке тем
        if self.background_image.contains(['/', '\\', ':']) || self.background_image.starts_with('.') {
            self.background_image.clear();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// множитель скругления углов: 0 — квадратные, 1 — как в тёмной, 2 — вдвое круглее
    pub radius: f32,
    /// "mono" / "sans" (Segoe UI) / "roboto" (Roboto, если установлен, иначе Segoe UI)
    pub font: String,
    /// "line" (Segoe MDL2) / "symbols" (обычные символы ↓ ⚙ ☰)
    pub icons: String,
    pub bg: Rgb,
    pub sidebar: Rgb,
    pub sidebar_line: Rgb,
    pub surface: Rgb,
    pub surface_hi: Rgb,
    pub pill: Rgb,
    pub border: Rgb,
    pub divider: Rgb,
    pub outline: Rgb,
    pub window: Rgb,
    pub field: Rgb,
    pub selection: Rgb,
    pub text: Rgb,
    pub text_hover: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub accent: Rgb,
    pub accent_hover: Rgb,
    pub on_accent: Rgb,
    pub toggle_off: Rgb,
    pub knob_off: Rgb,
    pub knob_on: Rgb,
    pub track: Rgb,
    pub ok: Rgb,
    pub ok_bg: Rgb,
    pub err: Rgb,
    pub warn: Rgb,
    pub warn_bg: Rgb,
    /// раскладка и форма (только в файле)
    pub layout: Layout,
    /// Material 3: seed и роли схемы (для Android — ColorScheme); у тем не на M3 — нет
    #[serde(skip_serializing_if = "Option::is_none")]
    pub m3: Option<M3>,
}

impl Default for Theme {
    fn default() -> Self {
        dark()
    }
}

impl Theme {
    /// Все цвета с подписями — для редактора (порядок = порядок на экране).
    pub fn colors_mut(&mut self) -> Vec<(&'static str, &mut Rgb)> {
        vec![
            ("фон", &mut self.bg),
            ("боковая панель", &mut self.sidebar),
            ("край боковой панели", &mut self.sidebar_line),
            ("карточки и кнопки", &mut self.surface),
            ("наведение", &mut self.surface_hi),
            ("маленькие кнопки", &mut self.pill),
            ("рамки", &mut self.border),
            ("разделители в карточках", &mut self.divider),
            ("рамка пустой очереди", &mut self.outline),
            ("всплывающие окна", &mut self.window),
            ("поля ввода", &mut self.field),
            ("выделение текста", &mut self.selection),
            ("текст", &mut self.text),
            ("текст при наведении", &mut self.text_hover),
            ("приглушённый текст", &mut self.muted),
            ("бледный текст", &mut self.faint),
            ("акцент (главная кнопка)", &mut self.accent),
            ("акцент при наведении", &mut self.accent_hover),
            ("текст на акценте", &mut self.on_accent),
            ("тумблер выключен", &mut self.toggle_off),
            ("кружок тумблера выкл.", &mut self.knob_off),
            ("кружок тумблера вкл.", &mut self.knob_on),
            ("полоса прогресса", &mut self.track),
            ("готово", &mut self.ok),
            ("фон зелёных плашек", &mut self.ok_bg),
            ("ошибка", &mut self.err),
            ("внимание", &mut self.warn),
            ("фон жёлтых плашек", &mut self.warn_bg),
        ]
    }

    pub fn is_light(&self) -> bool {
        self.bg.luma() > 0.5
    }

    /// Привести к допустимым значениям (после чтения чужого файла).
    pub fn sanitize(&mut self) {
        if !self.radius.is_finite() {
            self.radius = 1.0;
        }
        self.radius = self.radius.clamp(0.0, 3.0);
        if !FONTS.iter().any(|(k, _)| *k == self.font) {
            self.font = "mono".into();
        }
        if !ICON_SETS.iter().any(|(k, _)| *k == self.icons) {
            self.icons = "line".into();
        }
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            self.name = "без названия".into();
        }
        self.layout.sanitize();
    }
}

/// Текущая тёмная тема (по умолчанию) — в духе cobalt.tools.
pub fn dark() -> Theme {
    Theme {
        name: "тёмная".into(),
        radius: 1.0,
        font: "mono".into(),
        icons: "line".into(),
        bg: Rgb::hex(0x000000),
        sidebar: Rgb::hex(0x0b0b0b),
        sidebar_line: Rgb::hex(0x161616),
        surface: Rgb::hex(0x151515),
        surface_hi: Rgb::hex(0x222222),
        pill: Rgb::hex(0x1b1b1b),
        border: Rgb::hex(0x2a2a2a),
        divider: Rgb::hex(0x202020),
        outline: Rgb::hex(0x1c1c1c),
        window: Rgb::hex(0x101010),
        field: Rgb::hex(0x0d0d0d),
        selection: Rgb::hex(0x444444),
        text: Rgb::hex(0xe8e8e8),
        text_hover: Rgb::hex(0xffffff),
        muted: Rgb::hex(0x8c8c8c),
        faint: Rgb::hex(0x555555),
        accent: Rgb::hex(0xf2f2f2),
        accent_hover: Rgb::hex(0xffffff),
        on_accent: Rgb::hex(0x000000),
        toggle_off: Rgb::hex(0x3a3a3a),
        knob_off: Rgb::hex(0x9a9a9a),
        knob_on: Rgb::hex(0x000000),
        track: Rgb::hex(0x2c2c2c),
        ok: Rgb::hex(0x74d680),
        ok_bg: Rgb::hex(0x101610),
        err: Rgb::hex(0xf06e6e),
        warn: Rgb::hex(0xe8b44c),
        warn_bg: Rgb::hex(0x1d180c),
        m3: None,
        layout: Layout::default(),
    }
}

/// Светлая с синим акцентом.
pub fn light() -> Theme {
    Theme {
        name: "светлая".into(),
        radius: 1.0,
        font: "mono".into(),
        icons: "line".into(),
        bg: Rgb::hex(0xf6f6f4),
        sidebar: Rgb::hex(0xecebe8),
        sidebar_line: Rgb::hex(0xdddcd8),
        surface: Rgb::hex(0xffffff),
        surface_hi: Rgb::hex(0xe9e9e6),
        pill: Rgb::hex(0xf3f3f1),
        border: Rgb::hex(0xd6d5d1),
        divider: Rgb::hex(0xe8e7e3),
        outline: Rgb::hex(0xdcdbd7),
        window: Rgb::hex(0xffffff),
        field: Rgb::hex(0xf0efec),
        selection: Rgb::hex(0xc9d7f7),
        text: Rgb::hex(0x1a1a1a),
        text_hover: Rgb::hex(0x000000),
        muted: Rgb::hex(0x5c5c5c),
        faint: Rgb::hex(0x7a7a77),
        accent: Rgb::hex(0x3b6cf0),
        accent_hover: Rgb::hex(0x2c58d4),
        on_accent: Rgb::hex(0xffffff),
        toggle_off: Rgb::hex(0xcfcfcb),
        knob_off: Rgb::hex(0xffffff),
        knob_on: Rgb::hex(0xffffff),
        track: Rgb::hex(0xe0dfdb),
        ok: Rgb::hex(0x2e9a45),
        ok_bg: Rgb::hex(0xe6f4e8),
        err: Rgb::hex(0xd0433f),
        warn: Rgb::hex(0xa86d12),
        warn_bg: Rgb::hex(0xfbf0da),
        m3: None,
        layout: Layout::default(),
    }
}

/// Цветная: тёмно-фиолетовая с оранжевым акцентом, обычный шрифт, круглые углы.
pub fn sunset() -> Theme {
    Theme {
        name: "закат".into(),
        radius: 1.6,
        font: "sans".into(),
        icons: "line".into(),
        bg: Rgb::hex(0x1a1025),
        sidebar: Rgb::hex(0x140c1d),
        sidebar_line: Rgb::hex(0x2a1a38),
        surface: Rgb::hex(0x2a1b3a),
        surface_hi: Rgb::hex(0x3a2650),
        pill: Rgb::hex(0x30203f),
        border: Rgb::hex(0x4a3260),
        divider: Rgb::hex(0x3a2650),
        outline: Rgb::hex(0x3a2650),
        window: Rgb::hex(0x22152f),
        field: Rgb::hex(0x160d20),
        selection: Rgb::hex(0x6a3a6a),
        text: Rgb::hex(0xf5e9ff),
        text_hover: Rgb::hex(0xffffff),
        muted: Rgb::hex(0xb39cc8),
        faint: Rgb::hex(0x6f5a85),
        accent: Rgb::hex(0xff8a5c),
        accent_hover: Rgb::hex(0xffa47e),
        on_accent: Rgb::hex(0x1a1025),
        toggle_off: Rgb::hex(0x4a3260),
        knob_off: Rgb::hex(0xb39cc8),
        knob_on: Rgb::hex(0x1a1025),
        track: Rgb::hex(0x3a2650),
        ok: Rgb::hex(0x7ee0a0),
        ok_bg: Rgb::hex(0x1b2a26),
        err: Rgb::hex(0xff6b8a),
        warn: Rgb::hex(0xffc861),
        warn_bg: Rgb::hex(0x2e2020),
        m3: None,
        layout: Layout::default(),
    }
}

/// Совсем другая: зелёный терминал, квадратные углы.
pub fn terminal() -> Theme {
    Theme {
        name: "терминал".into(),
        radius: 0.0,
        font: "mono".into(),
        icons: "line".into(),
        bg: Rgb::hex(0x050a05),
        sidebar: Rgb::hex(0x081008),
        sidebar_line: Rgb::hex(0x1a3a1a),
        surface: Rgb::hex(0x0a160a),
        surface_hi: Rgb::hex(0x123012),
        pill: Rgb::hex(0x0c1a0c),
        border: Rgb::hex(0x1f4a1f),
        divider: Rgb::hex(0x143014),
        outline: Rgb::hex(0x1a3a1a),
        window: Rgb::hex(0x081008),
        field: Rgb::hex(0x030703),
        selection: Rgb::hex(0x1f5a1f),
        text: Rgb::hex(0x7dff8a),
        text_hover: Rgb::hex(0xc8ffd0),
        muted: Rgb::hex(0x3fae4c),
        faint: Rgb::hex(0x2a6a30),
        accent: Rgb::hex(0x7dff8a),
        accent_hover: Rgb::hex(0xc8ffd0),
        on_accent: Rgb::hex(0x031003),
        toggle_off: Rgb::hex(0x1f4a1f),
        knob_off: Rgb::hex(0x3fae4c),
        knob_on: Rgb::hex(0x031003),
        track: Rgb::hex(0x143014),
        ok: Rgb::hex(0x7dff8a),
        ok_bg: Rgb::hex(0x0c200c),
        err: Rgb::hex(0xff5f56),
        warn: Rgb::hex(0xffd75f),
        warn_bg: Rgb::hex(0x1c1a08),
        m3: None,
        layout: Layout::default(),
    }
}

/// Неон: навигация сверху с названием, карточки-рамки со свечением, градиент, контурные кнопки.
pub fn neon() -> Theme {
    Theme {
        name: "неон".into(),
        radius: 0.6,
        font: "mono".into(),
        icons: "line".into(),
        bg: Rgb::hex(0x0b0820),
        sidebar: Rgb::hex(0x0e0a26),
        sidebar_line: Rgb::hex(0x2c1f5c),
        surface: Rgb::hex(0x140f33),
        surface_hi: Rgb::hex(0x231a52),
        pill: Rgb::hex(0x120d2c),
        border: Rgb::hex(0x4b2f8f),
        divider: Rgb::hex(0x261c55),
        outline: Rgb::hex(0x3a2a78),
        window: Rgb::hex(0x120d2e),
        field: Rgb::hex(0x08061a),
        selection: Rgb::hex(0x5a2a8a),
        text: Rgb::hex(0xeae6ff),
        text_hover: Rgb::hex(0xffffff),
        muted: Rgb::hex(0xa79fd6),
        faint: Rgb::hex(0x6e66a3),
        accent: Rgb::hex(0xff3df0),
        accent_hover: Rgb::hex(0xff7af5),
        on_accent: Rgb::hex(0x14001a),
        toggle_off: Rgb::hex(0x2c2160),
        knob_off: Rgb::hex(0xa79fd6),
        knob_on: Rgb::hex(0x14001a),
        track: Rgb::hex(0x261c55),
        ok: Rgb::hex(0x3dffc5),
        ok_bg: Rgb::hex(0x0b2228),
        err: Rgb::hex(0xff4f7b),
        warn: Rgb::hex(0xffd23d),
        warn_bg: Rgb::hex(0x231a14),
        m3: None,
        layout: Layout {
            nav: "top".into(),
            header: "name".into(),
            cards: "outline".into(),
            shadow: "glow".into(),
            buttons: "outline".into(),
            background: vec![Rgb::hex(0x0b0820), Rgb::hex(0x1a0b2e), Rgb::hex(0x061428)],
            bold_headings: true,
            caps: "upper".into(),
            column: 700.0,
            ..Layout::default()
        },
    }
}

/// Облако: светлая и мягкая — навигация снизу одними значками, просторно, круглые плоские кнопки, тени.
pub fn cloud() -> Theme {
    Theme {
        name: "облако".into(),
        radius: 2.0,
        font: "sans".into(),
        icons: "line".into(),
        bg: Rgb::hex(0xf3f1fb),
        sidebar: Rgb::hex(0xffffff),
        sidebar_line: Rgb::hex(0xe4e0f3),
        surface: Rgb::hex(0xffffff),
        surface_hi: Rgb::hex(0xece8fb),
        pill: Rgb::hex(0xf1eefc),
        border: Rgb::hex(0xdcd6f0),
        divider: Rgb::hex(0xeeebf8),
        outline: Rgb::hex(0xd9d3ef),
        window: Rgb::hex(0xffffff),
        field: Rgb::hex(0xf6f4fd),
        selection: Rgb::hex(0xd6cdfb),
        text: Rgb::hex(0x24213a),
        text_hover: Rgb::hex(0x000000),
        muted: Rgb::hex(0x5d5878),
        faint: Rgb::hex(0x7f7a99),
        accent: Rgb::hex(0x7b61ff),
        accent_hover: Rgb::hex(0x6448f0),
        on_accent: Rgb::hex(0xffffff),
        toggle_off: Rgb::hex(0xd9d3ef),
        knob_off: Rgb::hex(0xffffff),
        knob_on: Rgb::hex(0xffffff),
        track: Rgb::hex(0xe4e0f3),
        ok: Rgb::hex(0x22955a),
        ok_bg: Rgb::hex(0xe5f6ec),
        err: Rgb::hex(0xd23f5e),
        warn: Rgb::hex(0x9a6a00),
        warn_bg: Rgb::hex(0xfbf1d9),
        m3: None,
        layout: Layout {
            nav: "bottom".into(),
            nav_labels: false,
            header: "none".into(),
            shadow: "shadow".into(),
            density: 1.15,
            buttons: "flat".into(),
            background: vec![Rgb::hex(0xf7f0ff), Rgb::hex(0xeef3ff)],
            font_scale: 1.08,
            ..Layout::default()
        },
    }
}

/// Бумага: газетная — широкая левая панель со строками, квадратные углы, прямоугольные кнопки, прописные заголовки.
pub fn paper() -> Theme {
    Theme {
        name: "бумага".into(),
        radius: 0.0,
        font: "mono".into(),
        icons: "line".into(),
        bg: Rgb::hex(0xf2ede1),
        sidebar: Rgb::hex(0xe8e1d0),
        sidebar_line: Rgb::hex(0x1b1b1b),
        surface: Rgb::hex(0xfbf8f0),
        surface_hi: Rgb::hex(0xe9e2d0),
        pill: Rgb::hex(0xf2ede1),
        border: Rgb::hex(0x1b1b1b),
        divider: Rgb::hex(0xcfc6b0),
        outline: Rgb::hex(0x1b1b1b),
        window: Rgb::hex(0xfbf8f0),
        field: Rgb::hex(0xffffff),
        selection: Rgb::hex(0xf5c4b8),
        text: Rgb::hex(0x141414),
        text_hover: Rgb::hex(0x000000),
        muted: Rgb::hex(0x4f4a40),
        faint: Rgb::hex(0x6f695c),
        accent: Rgb::hex(0xd8361a),
        accent_hover: Rgb::hex(0xb52a10),
        on_accent: Rgb::hex(0xffffff),
        toggle_off: Rgb::hex(0xc9c0aa),
        knob_off: Rgb::hex(0xfbf8f0),
        knob_on: Rgb::hex(0xffffff),
        track: Rgb::hex(0xd9d0bb),
        ok: Rgb::hex(0x1f7a3a),
        ok_bg: Rgb::hex(0xe3eedb),
        err: Rgb::hex(0xc0261a),
        warn: Rgb::hex(0x8a5a00),
        warn_bg: Rgb::hex(0xf5e7c4),
        m3: None,
        layout: Layout {
            nav_width: 190.0,
            header: "name".into(),
            cards: "outline".into(),
            buttons: "rect".into(),
            density: 0.95,
            font_scale: 1.04,
            bold_headings: true,
            caps: "upper".into(),
            column: 720.0,
            ..Layout::default()
        },
    }
}

/// Тема Material 3 из схемы: роли M3 → цвета окна, форма — по шкале M3.
/// Соответствие полей ролям — в README (раздел «Material 3»).
pub fn from_m3(name: &str, m: &M3) -> Theme {
    let s = m.roles();
    let dark = m.dark;
    let tone = |p: m3::Palette, light: f64, dark_t: f64| p.tone(if dark { dark_t } else { light });
    let (ok, warn) = (m3::success(), m3::warning());
    Theme {
        name: name.into(),
        radius: 1.0,
        font: "roboto".into(),
        icons: "line".into(),
        bg: s.surface,
        sidebar: s.surface,
        sidebar_line: s.surface,
        surface: s.surface_container,
        surface_hi: s.surface_container_highest,
        pill: s.secondary_container,
        border: s.outline,
        divider: s.outline_variant,
        outline: s.outline_variant,
        window: s.surface_container_high,
        field: s.surface_container_highest,
        selection: s.primary_container,
        text: s.on_surface,
        text_hover: s.on_surface,
        muted: s.on_surface_variant,
        faint: s.outline,
        accent: s.primary,
        accent_hover: m3::blend(s.primary, s.on_primary, 0.08),
        on_accent: s.on_primary,
        toggle_off: s.surface_container_highest,
        knob_off: s.outline,
        knob_on: s.on_primary,
        track: s.secondary_container,
        ok: tone(ok, 40.0, 80.0),
        ok_bg: tone(ok, 90.0, 30.0),
        err: s.error,
        warn: tone(warn, 40.0, 80.0),
        warn_bg: tone(warn, 90.0, 30.0),
        layout: Layout {
            nav_width: 80.0,
            nav_indicator: "pill".into(),
            buttons: "tonal".into(),
            button_radius: 100.0,
            field_radius: 4.0,
            switch: "m3".into(),
            density: 1.1,
            padding: 1.15,
            font_scale: 1.05,
            bold_headings: true,
            ..Layout::default()
        },
        m3: Some(M3 { seed: m.seed, dark, scheme: Some(s) }),
    }
}

/// Material 3 из базового seed (#6750a4), тёмная и светлая.
pub fn material(dark: bool) -> Theme {
    let m = M3 { seed: m3::BASELINE_SEED, dark, scheme: None };
    from_m3(if dark { "material 3" } else { "material 3 светлая" }, &m)
}

/// Встроенные темы: ключ в настройках ("" — тёмная) и сама тема.
pub fn builtins() -> Vec<(&'static str, Theme)> {
    vec![
        ("", dark()),
        ("light", light()),
        ("sunset", sunset()),
        ("terminal", terminal()),
        ("neon", neon()),
        ("cloud", cloud()),
        ("paper", paper()),
        ("material", material(true)),
        ("material-light", material(false)),
    ]
}

/// Убрать комментарии `// …` и `/* … */` (вне строк) — в файлах тем их можно писать.
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    let mut in_str = false;
    while let Some(c) = it.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = it.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, it.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in it.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                it.next();
                let mut prev = ' ';
                for n in it.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Пример темы со всеми полями и пояснениями — кладётся в папку тем.
pub const EXAMPLE_FILE: &str = "_пример — все поля.json";
pub const EXAMPLE: &str = include_str!("theme_example.json");

/// Положить в папку тем пример со всеми полями (если его там ещё нет).
pub fn ensure_example(dir: &Path) {
    let p = dir.join(EXAMPLE_FILE);
    if !p.exists() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(p, EXAMPLE);
    }
}

pub const FILE_PREFIX: &str = "file:";

/// Папка своих тем.
pub fn dir() -> PathBuf {
    data_dir().join("themes")
}

/// Прочитать тему из файла.
pub fn read_from(path: &Path) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("не прочитать {}: {e}", path.display()))?;
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let bad = |e: serde_json::Error| {
        // «expected `,` at line 5 column 3» → понятнее по-русски
        let m = e.to_string().replace(" at line ", " — строка ").replace(" column ", ", столбец ");
        format!("«{file}» не прочитан: {m}")
    };
    let v: serde_json::Value = serde_json::from_str(&strip_comments(text.trim_start_matches('\u{feff}'))).map_err(bad)?;
    let named = v.get("name").and_then(|n| n.as_str()).is_some_and(|s| !s.trim().is_empty());
    // Тема на Material 3: всё, что не задано в файле, — из ролей схемы (seed → роли → цвета окна).
    let v = match v.get("m3") {
        Some(m) if v.is_object() => {
            let m: M3 = serde_json::from_value(m.clone()).map_err(bad)?;
            let mut base = serde_json::to_value(from_m3("", &m)).map_err(bad)?;
            merge(&mut base, v);
            base
        }
        _ => v,
    };
    let mut t: Theme = serde_json::from_value(v).map_err(bad)?;
    if !named {
        t.name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    t.sanitize();
    Ok(t)
}

/// Наложить значения из файла поверх готовых (вложенные объекты — по полям).
fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, o) => *b = o,
    }
}

/// Записать тему в файл (папка создаётся).
pub fn write_to(path: &Path, t: &Theme) -> Result<(), String> {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let text = serde_json::to_string_pretty(t).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("не записать {}: {e}", path.display()))
}

/// Свои темы из папки: (имя файла без .json, тема), по названию.
pub fn list_custom_in(dir: &Path) -> Vec<(String, Theme)> {
    list_custom_report(dir).0
}

/// Свои темы из папки и ошибки чтения файлов, которые не удалось прочитать.
pub fn list_custom_report(dir: &Path) -> (Vec<(String, Theme)>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("json")) != Some(true) {
                continue;
            }
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            match read_from(&p) {
                Ok(t) => out.push((stem, t)),
                Err(e) => errors.push(e),
            }
        }
    }
    out.sort_by_key(|(_, t)| t.name.to_lowercase());
    errors.sort();
    (out, errors)
}

pub fn list_custom() -> (Vec<(String, Theme)>, Vec<String>) {
    list_custom_report(&dir())
}

/// Имя файла из названия темы: без запрещённых в Windows знаков.
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_matches('.').trim().to_string();
    let s: String = s.chars().take(60).collect();
    if s.is_empty() { "тема".into() } else { s }
}

/// Свободное имя файла для темы в папке: «моя тема», «моя тема 2»…
pub fn free_stem_in(dir: &Path, name: &str) -> String {
    let base = slug(name);
    let mut stem = base.clone();
    let mut n = 2;
    while dir.join(format!("{stem}.json")).exists() {
        stem = format!("{base} {n}");
        n += 1;
    }
    stem
}

/// Тема по ключу из настроек; нет такой — тёмная.
pub fn load(key: &str) -> Theme {
    if let Some(stem) = key.strip_prefix(FILE_PREFIX) {
        return read_from(&dir().join(format!("{stem}.json"))).unwrap_or_else(|_| dark());
    }
    builtins().into_iter().find(|(k, _)| *k == key).map(|(_, t)| t).unwrap_or_else(dark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_hex() {
        assert_eq!(Rgb::parse("#1a2B3c"), Some(Rgb([0x1a, 0x2b, 0x3c])));
        assert_eq!(Rgb::parse("fff"), Some(Rgb([255, 255, 255])));
        assert_eq!(Rgb::parse("#12345"), None);
        assert_eq!(Rgb::hex(0x0b0c0d).to_hex(), "#0b0c0d");
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("vgd-theme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut t = sunset();
        t.name = "мой: закат?".into();
        t.accent = Rgb::hex(0x123456);
        let stem = free_stem_in(&dir, &t.name);
        assert_eq!(stem, "мой закат");
        let path = dir.join(format!("{stem}.json"));
        write_to(&path, &t).unwrap();
        assert_eq!(read_from(&path).unwrap(), t);
        assert_eq!(free_stem_in(&dir, &t.name), "мой закат 2");
        let list = list_custom_in(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "мой закат");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_file_falls_back_to_dark() {
        let dir = std::env::temp_dir().join(format!("vgd-theme-part-{}", std::process::id()));
        let path = dir.join("x.json");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(&path, r##"{"accent": "#ff0000", "radius": 99, "icons": "что-то"}"##).unwrap();
        let t = read_from(&path).unwrap();
        assert_eq!(t.accent, Rgb::hex(0xff0000));
        assert_eq!(t.bg, dark().bg);
        assert_eq!(t.radius, 3.0);
        assert_eq!(t.icons, "line");
        assert_eq!(t.name, "x");
        std::fs::write(&path, r##"{"bg": "красный"}"##).unwrap();
        assert!(read_from(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn example_has_every_field_and_parses() {
        let v: serde_json::Value = serde_json::from_str(&strip_comments(EXAMPLE)).unwrap();
        let t: Theme = serde_json::from_value(v.clone()).unwrap();
        let full = serde_json::to_value(dark()).unwrap();
        let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&v), keys(&full), "в примере должны быть все поля темы");
        assert_eq!(keys(&v["layout"]), keys(&full["layout"]), "и все поля layout");
        let mut s = t.clone();
        s.sanitize();
        assert_eq!(s, t, "значения примера — допустимые");
    }

    #[test]
    fn comments_and_strings() {
        let j = strip_comments("{\"a\": \"http://x\", // c\n /* b */ \"b\": \"q\\\"//\"}");
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["a"], "http://x");
        assert_eq!(v["b"], "q\"//");
    }

    #[test]
    fn m3_from_seed_file() {
        // В файле только seed — роли и цвета окна выводятся, явно заданное в файле остаётся.
        let dir = std::env::temp_dir().join(format!("vgd-theme-m3-{}", std::process::id()));
        let path = dir.join("m.json");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(&path, r##"{"name": "синяя", "m3": {"seed": "#00639b", "dark": false}, "text": "#101010"}"##).unwrap();
        let t = read_from(&path).unwrap();
        let roles = t.m3.as_ref().and_then(|m| m.scheme.clone()).expect("роли выведены");
        assert_eq!(t.bg, roles.surface);
        assert_eq!(t.accent, roles.primary);
        assert_eq!(t.text, Rgb::hex(0x101010));
        assert_eq!(t.layout.buttons, "tonal");
        assert!(t.is_light());
        // записали и прочитали — то же самое (роли сохраняются в файл для Android)
        write_to(&path, &t).unwrap();
        assert_eq!(read_from(&path).unwrap(), t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m3_baseline_close_to_spec() {
        // Базовая схема M3 (Material Theme Builder, seed #6750a4): primary тёмной — #d0bcff, светлой — #6750a4.
        let near = |a: Rgb, b: u32| {
            let b = Rgb::hex(b);
            let d = (0..3).map(|i| (a.0[i] as i32 - b.0[i] as i32).abs()).max().unwrap();
            assert!(d <= 28, "{} vs {}", a.to_hex(), b.to_hex());
        };
        let d = m3::Scheme::from_seed(m3::BASELINE_SEED, true);
        near(d.primary, 0xd0bcff);
        near(d.on_primary, 0x381e72);
        near(d.surface, 0x141218);
        near(d.on_surface, 0xe6e0e9);
        near(d.outline, 0x938f99);
        near(d.error, 0xf2b8b5);
        let l = m3::Scheme::from_seed(m3::BASELINE_SEED, false);
        near(l.primary, 0x6750a4);
        near(l.surface, 0xfef7ff);
        near(l.secondary_container, 0xe8def8);
        near(l.outline, 0x79747e);
    }

    #[test]
    fn builtins_are_sane() {
        for (_, t) in builtins() {
            let mut s = t.clone();
            s.sanitize();
            assert_eq!(s, t, "{}", t.name);
        }
    }

    #[test]
    fn old_settings_without_theme() {
        let s: crate::settings::Settings = serde_json::from_str(r#"{"playlist": false}"#).unwrap();
        assert_eq!(s.theme, "");
        assert_eq!(load(&s.theme), dark());
    }
}
