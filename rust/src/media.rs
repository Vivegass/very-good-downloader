//! ffmpeg: сведения о файле (без ffprobe — экономит 100 МБ в установщике) и запуск с прогрессом.

use std::path::Path;

use crate::appinfo::{command, tool};

#[derive(Debug, Default, Clone)]
pub struct MediaInfo {
    pub duration: f64,
    pub fps: f64,
    pub hdr: bool,
    pub has_audio: bool,
    pub has_video: bool,
}

/// Разбирает вывод `ffmpeg -i файл` (он пишет сведения в stderr и завершается с ошибкой — это нормально).
pub fn probe(path: &Path) -> MediaInfo {
    let out = command(tool("ffmpeg")).args(["-hide_banner", "-nostdin", "-i"]).arg(path).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
    parse_probe(&text)
}

pub fn parse_probe(text: &str) -> MediaInfo {
    let mut m = MediaInfo::default();
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("Duration:") {
            let d = rest.trim().split(',').next().unwrap_or("");
            m.duration = parse_hms(d).unwrap_or(0.0);
        } else if l.starts_with("Stream #") && l.contains(": Video:") {
            if m.has_video {
                continue; // берём первый видеопоток (обложки в MKV идут позже)
            }
            m.has_video = true;
            m.hdr = l.contains("smpte2084") || l.contains("arib-std-b67");
            // «…, 2560x1080 [SAR 1:1 DAR 64:27], 47.95 fps, 48 tbr, …»
            for part in l.split(',') {
                if let Some(v) = part.trim().strip_suffix(" fps") {
                    m.fps = parse_num(v).unwrap_or(0.0);
                }
            }
        } else if l.starts_with("Stream #") && l.contains(": Audio:") {
            m.has_audio = true;
        }
    }
    m
}

fn parse_num(v: &str) -> Option<f64> {
    let v = v.trim();
    if let Some(k) = v.strip_suffix('k') {
        return k.parse::<f64>().ok().map(|x| x * 1000.0);
    }
    v.parse().ok()
}

/// «00:01:02.50» → 62.5
pub fn parse_hms(s: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in s.trim().split(':') {
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(total)
}

pub fn fmt_bytes(n: f64) -> String {
    if n <= 0.0 {
        return String::new();
    }
    let mut n = n;
    for unit in ["Б", "КБ", "МБ", "ГБ"] {
        if n < 1024.0 {
            return format!("{n:.1} {unit}");
        }
        n /= 1024.0;
    }
    format!("{n:.1} ТБ")
}

pub fn fmt_eta(s: f64) -> String {
    if !s.is_finite() || s < 0.0 {
        return String::new();
    }
    let s = s as u64;
    let (h, m, sec) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_text() {
        let t = "Input #0, matroska,webm, from 'a.mkv':\n  Duration: 00:00:36.04, start: 0.000000, bitrate: 3000 kb/s\n  Stream #0:0(eng): Video: av1 (libdav1d) (Main), yuv420p10le(tv, bt2020nc/bt2020/smpte2084), 2560x1080, SAR 1:1 DAR 64:27, 47.95 fps, 48 tbr, 1k tbn (default)\n  Stream #0:1(eng): Audio: opus, 48000 Hz, stereo, fltp (default)\n";
        let m = parse_probe(t);
        assert!((m.duration - 36.04).abs() < 1e-6);
        assert!((m.fps - 47.95).abs() < 1e-6);
        assert!(m.hdr && m.has_audio && m.has_video);
    }
}
