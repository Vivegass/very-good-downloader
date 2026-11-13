//! Material Design 3: цветовая схема по ролям из одного seed-цвета (как «tonal spot» в M3).
//!
//! Роли названы как поля `androidx.compose.material3.ColorScheme` (camelCase в JSON), чтобы
//! Android-версия читала тот же файл темы и строила `ColorScheme` напрямую.
//!
//! Тональные палитры: тон = L* из CIELAB (в M3/HCT тон — тот же L*), оттенок и насыщенность —
//! из CIELAB LCh, а не CAM16, поэтому цвета близки к Material Theme Builder, но не совпадают
//! до единицы. Для переноса на Android важна не точность, а то, что обе версии берут роли из файла.

use super::Rgb;
use serde::{Deserialize, Serialize};

/// Роли цветовой схемы M3 (имена — как в Jetpack Compose `ColorScheme`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheme {
    pub primary: Rgb,
    pub on_primary: Rgb,
    pub primary_container: Rgb,
    pub on_primary_container: Rgb,
    pub inverse_primary: Rgb,
    pub secondary: Rgb,
    pub on_secondary: Rgb,
    pub secondary_container: Rgb,
    pub on_secondary_container: Rgb,
    pub tertiary: Rgb,
    pub on_tertiary: Rgb,
    pub tertiary_container: Rgb,
    pub on_tertiary_container: Rgb,
    pub background: Rgb,
    pub on_background: Rgb,
    pub surface: Rgb,
    pub on_surface: Rgb,
    pub surface_variant: Rgb,
    pub on_surface_variant: Rgb,
    pub surface_tint: Rgb,
    pub inverse_surface: Rgb,
    pub inverse_on_surface: Rgb,
    pub error: Rgb,
    pub on_error: Rgb,
    pub error_container: Rgb,
    pub on_error_container: Rgb,
    pub outline: Rgb,
    pub outline_variant: Rgb,
    pub scrim: Rgb,
    pub surface_bright: Rgb,
    pub surface_container: Rgb,
    pub surface_container_high: Rgb,
    pub surface_container_highest: Rgb,
    pub surface_container_low: Rgb,
    pub surface_container_lowest: Rgb,
    pub surface_dim: Rgb,
}

/// Раздел "m3" файла темы: seed-цвет, тёмная или светлая схема и (необязательно) сами роли.
/// Без "scheme" роли выводятся из seed; цвета окна (bg, text, accent…) — из ролей, если не заданы в файле явно.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct M3 {
    pub seed: Rgb,
    pub dark: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Scheme>,
}

impl Default for M3 {
    fn default() -> Self {
        M3 { seed: BASELINE_SEED, dark: true, scheme: None }
    }
}

impl M3 {
    /// Роли: заданные в файле или выведенные из seed.
    pub fn roles(&self) -> Scheme {
        self.scheme.clone().unwrap_or_else(|| Scheme::from_seed(self.seed, self.dark))
    }
}

/// Seed базовой схемы M3 (фиолетовая «baseline»).
pub const BASELINE_SEED: Rgb = Rgb::hex(0x6750a4);

// ---------- цвет: sRGB <-> CIELAB ----------

fn lin(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn delin(c: f64) -> f64 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

const WHITE: [f64; 3] = [0.95047, 1.0, 1.08883];

fn f_lab(t: f64) -> f64 {
    if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 }
}

fn f_inv(t: f64) -> f64 {
    let t3 = t * t * t;
    if t3 > 216.0 / 24389.0 { t3 } else { (116.0 * t - 16.0) / (24389.0 / 27.0) }
}

/// (L*, C, h°) цвета.
pub fn lch(c: Rgb) -> (f64, f64, f64) {
    let [r, g, b] = c.0.map(lin);
    let x = 0.4124 * r + 0.3576 * g + 0.1805 * b;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = 0.0193 * r + 0.1192 * g + 0.9505 * b;
    let (fx, fy, fz) = (f_lab(x / WHITE[0]), f_lab(y / WHITE[1]), f_lab(z / WHITE[2]));
    let (l, a, bb) = (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
    (l, a.hypot(bb), bb.atan2(a).to_degrees().rem_euclid(360.0))
}

/// Цвет по L*, C, h; None — вне sRGB.
fn from_lch(l: f64, c: f64, h: f64) -> Option<Rgb> {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let fy = (l + 16.0) / 116.0;
    let (x, y, z) = (WHITE[0] * f_inv(fy + a / 500.0), WHITE[1] * f_inv(fy), WHITE[2] * f_inv(fy - b / 200.0));
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let bl = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    let ok = |v: f64| (-0.0005..=1.0005).contains(&v);
    if !(ok(r) && ok(g) && ok(bl)) {
        return None;
    }
    let to = |v: f64| (delin(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
    Some(Rgb([to(r), to(g), to(bl)]))
}

/// Тональная палитра: один оттенок и насыщенность, тон 0..100.
#[derive(Clone, Copy)]
pub struct Palette {
    pub hue: f64,
    pub chroma: f64,
}

impl Palette {
    /// Цвет с тоном t (L*); насыщенность уменьшается, пока цвет не влезет в sRGB.
    pub fn tone(&self, t: f64) -> Rgb {
        if t <= 0.0 {
            return Rgb([0, 0, 0]);
        }
        if t >= 100.0 {
            return Rgb([255, 255, 255]);
        }
        let (mut lo, mut hi) = (0.0, self.chroma);
        if let Some(c) = from_lch(t, hi, self.hue) {
            return c;
        }
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if from_lch(t, mid, self.hue).is_some() { lo = mid } else { hi = mid }
        }
        from_lch(t, lo, self.hue).unwrap_or(Rgb([0, 0, 0]))
    }
}

fn pal_of(c: Rgb, chroma: f64) -> Palette {
    Palette { hue: lch(c).2, chroma }
}

/// Палитры «tonal spot» из seed: основная, вторичная, третичная (оттенок +60°), нейтральные, ошибка.
pub struct Palettes {
    pub primary: Palette,
    pub secondary: Palette,
    pub tertiary: Palette,
    pub neutral: Palette,
    pub neutral_variant: Palette,
    pub error: Palette,
}

impl Palettes {
    pub fn from_seed(seed: Rgb) -> Self {
        let (_, c, h) = lch(seed);
        Palettes {
            primary: Palette { hue: h, chroma: c.max(48.0) },
            secondary: Palette { hue: h, chroma: 16.0 },
            tertiary: Palette { hue: (h + 60.0).rem_euclid(360.0), chroma: 24.0 },
            neutral: Palette { hue: h, chroma: 4.0 },
            neutral_variant: Palette { hue: h, chroma: 8.0 },
            error: pal_of(Rgb::hex(0xb3261e), 84.0),
        }
    }
}

/// Дополнительные цвета вне ролей M3 (как «custom colors» в Theme Builder): успех и предупреждение.
pub fn success() -> Palette {
    pal_of(Rgb::hex(0x386a20), 48.0)
}

pub fn warning() -> Palette {
    pal_of(Rgb::hex(0x7c5800), 48.0)
}

impl Scheme {
    pub fn from_seed(seed: Rgb, dark: bool) -> Self {
        let p = Palettes::from_seed(seed);
        // тона ролей — по спецификации M3 (тёмная / светлая)
        let t = |light: f64, dark_t: f64| if dark { dark_t } else { light };
        let (pr, se, te, n, nv, er) = (p.primary, p.secondary, p.tertiary, p.neutral, p.neutral_variant, p.error);
        Scheme {
            primary: pr.tone(t(40.0, 80.0)),
            on_primary: pr.tone(t(100.0, 20.0)),
            primary_container: pr.tone(t(90.0, 30.0)),
            on_primary_container: pr.tone(t(10.0, 90.0)),
            inverse_primary: pr.tone(t(80.0, 40.0)),
            secondary: se.tone(t(40.0, 80.0)),
            on_secondary: se.tone(t(100.0, 20.0)),
            secondary_container: se.tone(t(90.0, 30.0)),
            on_secondary_container: se.tone(t(10.0, 90.0)),
            tertiary: te.tone(t(40.0, 80.0)),
            on_tertiary: te.tone(t(100.0, 20.0)),
            tertiary_container: te.tone(t(90.0, 30.0)),
            on_tertiary_container: te.tone(t(10.0, 90.0)),
            background: n.tone(t(98.0, 6.0)),
            on_background: n.tone(t(10.0, 90.0)),
            surface: n.tone(t(98.0, 6.0)),
            on_surface: n.tone(t(10.0, 90.0)),
            surface_variant: nv.tone(t(90.0, 30.0)),
            on_surface_variant: nv.tone(t(30.0, 80.0)),
            surface_tint: pr.tone(t(40.0, 80.0)),
            inverse_surface: n.tone(t(20.0, 90.0)),
            inverse_on_surface: n.tone(t(95.0, 20.0)),
            error: er.tone(t(40.0, 80.0)),
            on_error: er.tone(t(100.0, 20.0)),
            error_container: er.tone(t(90.0, 30.0)),
            on_error_container: er.tone(t(10.0, 90.0)),
            outline: nv.tone(t(50.0, 60.0)),
            outline_variant: nv.tone(t(80.0, 30.0)),
            scrim: n.tone(0.0),
            surface_bright: n.tone(t(98.0, 24.0)),
            surface_container: n.tone(t(94.0, 12.0)),
            surface_container_high: n.tone(t(92.0, 17.0)),
            surface_container_highest: n.tone(t(90.0, 22.0)),
            surface_container_low: n.tone(t(96.0, 10.0)),
            surface_container_lowest: n.tone(t(100.0, 4.0)),
            surface_dim: n.tone(t(87.0, 6.0)),
        }
    }
}

/// Смесь цветов: a + (b − a)·k (для слоя состояния «наведение» M3 — 8%).
pub fn blend(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Rgb([m(a.0[0], b.0[0]), m(a.0[1], b.0[1]), m(a.0[2], b.0[2])])
}
