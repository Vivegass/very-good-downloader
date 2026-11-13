//! Темы оформления окна: цвета, скругление углов, шрифт и набор значков.
//!
//! Встроенные темы зашиты в программу, свои лежат файлами JSON в
//! `%APPDATA%\VeryGoodDownloader\themes\` — одна тема = один файл, им можно делиться.
//! В файле можно указать не все поля: недостающие берутся из тёмной темы.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

use crate::appinfo::data_dir;

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
pub const FONTS: &[(&str, &str)] = &[("mono", "моноширинный"), ("sans", "обычный (segoe ui)")];
/// Набор значков.
pub const ICON_SETS: &[(&str, &str)] = &[("line", "тонкие"), ("symbols", "символы")];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// множитель скругления углов: 0 — квадратные, 1 — как в тёмной, 2 — вдвое круглее
    pub radius: f32,
    /// "mono" / "sans"
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
        muted: Rgb::hex(0x6b6b6b),
        faint: Rgb::hex(0xa3a3a0),
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
    }
}

/// Совсем другая: зелёный терминал, квадратные углы, значки-символы.
pub fn terminal() -> Theme {
    Theme {
        name: "терминал".into(),
        radius: 0.0,
        font: "mono".into(),
        icons: "symbols".into(),
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
    }
}

/// Встроенные темы: ключ в настройках ("" — тёмная) и сама тема.
pub fn builtins() -> Vec<(&'static str, Theme)> {
    vec![("", dark()), ("light", light()), ("sunset", sunset()), ("terminal", terminal())]
}

pub const FILE_PREFIX: &str = "file:";

/// Папка своих тем.
pub fn dir() -> PathBuf {
    data_dir().join("themes")
}

/// Прочитать тему из файла.
pub fn read_from(path: &Path) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("не прочитать {}: {e}", path.display()))?;
    let bad = |e: serde_json::Error| format!("{} — не тема: {e}", path.display());
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(bad)?;
    let named = v.get("name").and_then(|n| n.as_str()).is_some_and(|s| !s.trim().is_empty());
    let mut t: Theme = serde_json::from_value(v).map_err(bad)?;
    if !named {
        t.name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    t.sanitize();
    Ok(t)
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
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("json")) != Some(true) {
                continue;
            }
            if let (Some(stem), Ok(t)) = (p.file_stem().map(|s| s.to_string_lossy().into_owned()), read_from(&p)) {
                out.push((stem, t));
            }
        }
    }
    out.sort_by_key(|(_, t)| t.name.to_lowercase());
    out
}

pub fn list_custom() -> Vec<(String, Theme)> {
    list_custom_in(&dir())
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
    fn old_settings_without_theme() {
        let s: crate::settings::Settings = serde_json::from_str(r#"{"playlist": false}"#).unwrap();
        assert_eq!(s.theme, "");
        assert_eq!(load(&s.theme), dark());
    }
}
