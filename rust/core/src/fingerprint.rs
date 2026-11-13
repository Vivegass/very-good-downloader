//! Сравнение двух записей по звуку: тот ли это трек (а не кавер, ремикс или «sped up»).
//!
//! Звук → моно 11 025 Гц → кадры по 2048 отсчётов с шагом 512 → громкость (лог) в 24 полосах 200–4000 Гц.
//! Короткий отрывок (30 с с Deezer) двигается вдоль найденного трека, в каждом месте считается
//! корреляция спектрограмм (у каждой полосы вычтено среднее). Та же запись — 0,8–1,
//! другая песня — около 0–0,3; ускоренная («sped up») и кавер расходятся во времени и тоже дают мало.

use std::path::Path;

use crate::appinfo::{command, tool};

const RATE: u32 = 11_025;
const FRAME: usize = 2048;
const HOP: usize = 512;
const BANDS: usize = 24;

/// Сходство, начиная с которого считаем, что запись та же.
pub const SAME_MIN: f32 = 0.5;

/// Звук файла (или ссылки) моно 11 025 Гц, не длиннее `max_secs`.
pub fn decode(src: &Path, max_secs: u32) -> Option<Vec<f32>> {
    let out = command(tool("ffmpeg"))
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(src)
        .args(["-t", &max_secs.to_string(), "-vn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.len() < FRAME * 4 {
        return None;
    }
    Some(out.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}

/// Комплексное БПФ на месте (длина — степень двойки).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Спектрограмма: лог-громкость полос в каждом кадре.
pub fn fingerprint(samples: &[f32]) -> Vec<[f32; BANDS]> {
    if samples.len() < FRAME {
        return Vec::new();
    }
    // Границы полос (в номерах бинов БПФ), логарифмически от 200 до 4000 Гц.
    let edges: Vec<usize> = (0..=BANDS)
        .map(|i| {
            let f = 200.0 * (20f32).powf(i as f32 / BANDS as f32);
            ((f / RATE as f32 * FRAME as f32).round() as usize).min(FRAME / 2)
        })
        .collect();
    let window: Vec<f32> =
        (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME - 1) as f32).cos()).collect();
    let (mut re, mut im) = (vec![0f32; FRAME], vec![0f32; FRAME]);
    let mut out = Vec::with_capacity(samples.len() / HOP);
    let mut pos = 0;
    while pos + FRAME <= samples.len() {
        for i in 0..FRAME {
            re[i] = samples[pos + i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let mut e = [0f32; BANDS];
        for b in 0..BANDS {
            let s: f32 = (edges[b]..edges[b + 1].max(edges[b] + 1)).map(|k| re[k] * re[k] + im[k] * im[k]).sum();
            e[b] = (s + 1e-6).ln();
        }
        out.push(e);
        pos += HOP;
    }
    out
}

/// Лучшее сходство короткой спектрограммы с куском длинной (-1…1, 1 — одинаковые).
pub fn best_match(short: &[[f32; BANDS]], long: &[[f32; BANDS]]) -> Option<f32> {
    // Отрывок может быть чуть длиннее трека (другая версия) — сравниваем то, что влезает.
    let n = short.len().min(long.len());
    if n < 100 {
        return None;
    }
    let short = &short[..n];
    // Отрывок: у каждой полосы вычитаем среднее.
    let mut a = short.to_vec();
    for b in 0..BANDS {
        let m = a.iter().map(|f| f[b]).sum::<f32>() / n as f32;
        a.iter_mut().for_each(|f| f[b] -= m);
    }
    let na = a.iter().flat_map(|f| f.iter()).map(|x| x * x).sum::<f32>().sqrt();
    if na <= 1e-6 {
        return None;
    }
    // Суммы и суммы квадратов по полосам — чтобы в каждом окне длинной быстро вычесть среднее.
    let mut sum = vec![[0f64; BANDS]; long.len() + 1];
    let mut sq = vec![0f64; long.len() + 1];
    for (i, f) in long.iter().enumerate() {
        let mut row = sum[i];
        let mut q = sq[i];
        for b in 0..BANDS {
            row[b] += f[b] as f64;
            q += (f[b] as f64) * (f[b] as f64);
        }
        sum[i + 1] = row;
        sq[i + 1] = q;
    }
    let mut best = f32::MIN;
    for off in 0..=long.len() - n {
        // Σ a·b: у `a` среднее по полосам уже 0, поэтому среднее `b` вычитать не нужно.
        let mut dot = 0f32;
        for (x, y) in a.iter().zip(&long[off..off + n]) {
            for b in 0..BANDS {
                dot += x[b] * y[b];
            }
        }
        // |b - среднее|² = Σb² - n·Σ(среднее²)
        let mut nb2 = sq[off + n] - sq[off];
        for b in 0..BANDS {
            let s = sum[off + n][b] - sum[off][b];
            nb2 -= s * s / n as f64;
        }
        if nb2 <= 1e-9 {
            continue;
        }
        best = best.max(dot / (na * (nb2.sqrt() as f32)));
    }
    (best > f32::MIN).then_some(best)
}

/// Сходство отрывка-образца с найденным файлом. None — не удалось прочитать звук.
pub fn compare(sample: &Path, candidate: &Path) -> Option<f32> {
    let a = fingerprint(&decode(sample, 60)?);
    let b = fingerprint(&decode(candidate, 1800)?);
    best_match(&a, &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// «Музыка»: смена нот каждые 0,25 с, свой набор нот у каждого `seed`.
    fn tune(seed: u64, secs: f32, speed: f32) -> Vec<f32> {
        let n = (secs * RATE as f32) as usize;
        let mut x = seed;
        let notes: Vec<f32> = (0..400)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                220.0 * 2f32.powf((x % 24) as f32 / 12.0)
            })
            .collect();
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32 * speed;
                let f = notes[(t / 0.25) as usize % notes.len()];
                (2.0 * std::f32::consts::PI * f * t).sin() * 0.3 + (2.0 * std::f32::consts::PI * f * 1.5 * t).sin() * 0.15
            })
            .collect()
    }

    #[test]
    fn same_and_different() {
        let track = tune(7, 90.0, 1.0);
        // Отрывок с 31,3 с (не по сетке кадров) с шумом — та же запись.
        let start = (31.3 * RATE as f32) as usize;
        let mut noise = 1u64;
        let clip: Vec<f32> = track[start..start + 30 * RATE as usize]
            .iter()
            .map(|s| {
                noise = noise.wrapping_mul(6364136223846793005).wrapping_add(1);
                s + ((noise >> 33) as f32 / (1u64 << 31) as f32 - 0.5) * 0.02
            })
            .collect();
        let same = best_match(&fingerprint(&clip), &fingerprint(&track)).unwrap();
        assert!(same > 0.8, "та же запись: {same}");
        let other = best_match(&fingerprint(&clip), &fingerprint(&tune(99, 90.0, 1.0))).unwrap();
        assert!(other < SAME_MIN, "другая запись: {other}");
        // «sped up» — та же музыка в 1,25 раза быстрее.
        let fast = best_match(&fingerprint(&clip), &fingerprint(&tune(7, 90.0, 1.25))).unwrap();
        assert!(fast < SAME_MIN, "ускоренная: {fast}");
    }
}
