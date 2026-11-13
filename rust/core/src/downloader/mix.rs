//! Микс или альбом одним видео → отдельные треки по главам YouTube, таймкодам в описании
//! или в закреплённом (верхнем) комментарии. Звук качается один раз и режется ffmpeg.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{fail, first_line, note_source, rand_u64, str_of, DlError, Downloader, Entry, R};
use crate::appinfo::tool;
use crate::music::music_root;
use crate::sort::safe_name;

/// Начало трека (секунды) и его название.
type Mark = (f64, String);

/// Главы из сведений yt-dlp (YouTube сам делает их из таймкодов в описании).
fn chapters_of(info: &Value) -> Vec<Mark> {
    info.get("chapters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| Some((c.get("start_time")?.as_f64()?, str_of(c, &["title"]))))
        .collect()
}

/// «1:02:03» / «07:28» / «0:00» → секунды.
fn parse_time(t: &str) -> Option<f64> {
    let parts: Vec<&str> = t.split(':').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || p.len() > 2 || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    // Минуты и секунды после первого поля — ровно две цифры («7:28», «1:07:28»).
    if parts[1..].iter().any(|p| p.len() != 2) {
        return None;
    }
    let n: Vec<f64> = parts.iter().map(|p| p.parse().unwrap_or(0.0)).collect();
    Some(n.iter().fold(0.0, |acc, v| acc * 60.0 + v))
}

/// Таймкоды из текста (описание или комментарий): строка с временем = трек, остальное в строке — название.
/// Нужно хотя бы 3 трека, время должно расти.
pub fn parse_timestamps(text: &str) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // Первое «слово», похожее на время; скобки вокруг него («[00:00]», «(07:28)») не мешают.
        let mut found = None;
        for (i, w) in line.split_whitespace().enumerate() {
            let core = w.trim_matches(|c: char| "[]()|-–—.,:".contains(c));
            if let Some(t) = parse_time(core) {
                found = Some((i, w, t));
                break;
            }
            if i >= 2 {
                break;
            }
        }
        let Some((_, word, t)) = found else { continue };
        let pos = line.find(word).unwrap_or(0);
        let rest = format!("{} {}", &line[..pos], &line[pos + word.len()..]);
        let title = rest
            .trim()
            .trim_matches(|c: char| c.is_whitespace() || "-–—|:.)]([•·~".contains(c))
            .to_string();
        // Нумерация в начале: «01. », «1) »
        let title = strip_number(&title);
        if out.last().is_some_and(|(prev, _)| t <= *prev) {
            continue;
        }
        out.push((t, if title.is_empty() { format!("Трек {}", out.len() + 1) } else { title }));
    }
    if out.len() >= 3 { out } else { Vec::new() }
}

/// «01. Название» / «1) Название» → «Название».
fn strip_number(s: &str) -> String {
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let rest = &s[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")).or_else(|| rest.strip_prefix(" - ")) {
            return r.trim().to_string();
        }
    }
    s.to_string()
}

/// «Исполнитель - Название» в названии главы, иначе исполнитель — автор видео.
fn split_artist(name: &str, default: &str) -> (String, String) {
    for sep in [" - ", " – ", " — "] {
        if let Some((a, t)) = name.split_once(sep) {
            if !a.trim().is_empty() && !t.trim().is_empty() {
                return (a.trim().to_string(), t.trim().to_string());
            }
        }
    }
    (default.to_string(), name.to_string())
}

impl Downloader {
    /// Таймкоды из комментариев: закреплённый — первым, иначе тот, где их больше всего.
    fn marks_from_comments(&mut self, url: &str) -> R<Vec<Mark>> {
        self.p.message = "Ищу таймкоды в комментариях…".into();
        self.emit();
        let args: Vec<String> = vec![
            "-j".into(), "--no-playlist".into(), "--skip-download".into(), "--write-comments".into(),
            "--extractor-args".into(), "youtube:max_comments=40,all,0,0;comment_sort=top".into(),
        ];
        let info = match self.json_with_login_retry(&args, url) {
            Ok(v) => v,
            Err(DlError::Cancelled(m)) => return Err(DlError::Cancelled(m)),
            Err(DlError::Failed(_)) => return Ok(Vec::new()),
        };
        let mut best: Vec<Mark> = Vec::new();
        for c in info.get("comments").and_then(Value::as_array).into_iter().flatten() {
            let marks = parse_timestamps(&str_of(c, &["text"]));
            if c.get("is_pinned").and_then(Value::as_bool) == Some(true) && !marks.is_empty() {
                return Ok(marks);
            }
            if marks.len() > best.len() {
                best = marks;
            }
        }
        Ok(best)
    }

    pub(super) fn mix_tracks(&mut self, info: &Value, e: &Entry) -> R<()> {
        let title = str_of(info, &["title"]).if_empty_str(&e.id);
        let duration = info.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
        let page = str_of(info, &["webpage_url", "original_url"]).if_empty_str(&e.url);
        let (mut marks, mut origin) = (chapters_of(info), "главы YouTube");
        if marks.len() < 2 {
            (marks, origin) = (parse_timestamps(&str_of(info, &["description"])), "таймкоды в описании");
        }
        if marks.len() < 2 {
            (marks, origin) = (self.marks_from_comments(&page)?, "таймкоды в комментарии");
        }
        if marks.len() < 2 {
            return fail("в видео нет глав и таймкодов (ни в описании, ни в комментариях) — разбить не на что");
        }
        let n = marks.len();
        self.log(format!("«{title}»: {n} трек(ов), {origin}."));

        // 1. Звук целиком (и обложка) — во временную папку.
        let root = music_root(&self.s);
        let dir = root.join(safe_name(&title, 80));
        let parts = root.join(".parts");
        std::fs::create_dir_all(&dir).map_err(|err| DlError::Failed(format!("Не удалось создать папку: {err}")))?;
        crate::appinfo::hidden_dir(&parts);
        let tag = format!("mix-{}", rand_u64());
        let json_path = parts.join(format!("{tag}.json"));
        std::fs::write(&json_path, serde_json::to_vec(info).unwrap_or_default())
            .map_err(|err| DlError::Failed(format!("временный файл: {err}")))?;
        let mut args: Vec<String> = vec![
            "--load-info-json".into(), json_path.to_string_lossy().into_owned(),
            "-f".into(), "bestaudio/best".into(),
            "-P".into(), format!("home:{}", parts.display()),
            "-P".into(), format!("temp:{}", parts.display()),
            "-o".into(), format!("{tag}.%(ext)s"),
            "--retries".into(), "15".into(), "--fragment-retries".into(), "30".into(),
            "--write-thumbnail".into(), "--convert-thumbnails".into(), "jpg".into(),
            "--ppa".into(), r#"ThumbnailsConvertor+ffmpeg_o:-vf "crop=min(iw\,ih):min(iw\,ih)""#.into(),
            "--newline".into(), "--progress".into(),
            "--progress-template".into(),
            "download:VGDP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|none|%(info.format_note)s|%(info.format_id)s".into(),
            "--print".into(), "after_move:VGDF|%(filepath)s".into(),
        ];
        if self.use_cookies {
            args.extend(self.cookie_args());
        }
        self.p.title = title.clone();
        self.p.stage = "audio".into();
        self.p.message = "Скачиваю звук целиком…".into();
        self.emit();
        let mut audio: Option<String> = None;
        let mut finished = 0;
        let r = self.ytdlp(&args, |me, line| {
            if let Some(rest) = line.strip_prefix("VGDF|") {
                audio = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("VGDP|") {
                me.on_progress_line(rest, &Value::Null, &mut finished);
            }
        });
        let _ = std::fs::remove_file(&json_path);
        let r = r?;
        let audio = match audio {
            Some(a) if r.ok && Path::new(&a).exists() => PathBuf::from(a),
            _ => return fail(r.errors.last().cloned().unwrap_or_else(|| "звук не скачался".into())),
        };
        let thumb = audio.with_extension("jpg");
        let cover = dir.join("cover.jpg");
        if thumb.exists() {
            let _ = std::fs::copy(&thumb, &cover);
        }

        // 2. Формат треков: как в настройках музыки; «как есть» — без перекодирования.
        let src_ext = audio.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (ext, codec): (&str, Vec<&str>) = match self.s.music_format.as_str() {
            "mp3" => ("mp3", vec!["-c:a", "libmp3lame", "-b:a", "320k"]),
            "m4a" => ("m4a", vec!["-c:a", "aac", "-b:a", "256k"]),
            "flac" => ("flac", vec!["-c:a", "flac"]),
            _ => match src_ext.as_str() {
                "webm" | "opus" | "ogg" => ("opus", vec!["-c:a", "copy"]),
                "m4a" | "mp4" => ("m4a", vec!["-c:a", "copy"]),
                _ => ("mp3", vec!["-c:a", "libmp3lame", "-b:a", "320k"]),
            },
        };
        // В .opus ffmpeg обложку не вшивает — она лежит рядом (cover.jpg).
        let embed_cover = cover.exists() && ext != "opus";
        let uploader = str_of(info, &["artist", "uploader", "channel"]).trim_end_matches(" - Topic").to_string();
        let width = n.to_string().len().max(2);
        let mut m3u = Vec::new();
        let mut done = 0;

        // 3. Режем.
        for (k, (start, name)) in marks.iter().enumerate() {
            self.check_cancel()?;
            let end = marks.get(k + 1).map(|m| m.0).or((duration > 0.0).then_some(duration));
            let (artist, track) = split_artist(name, &uploader);
            let base = if artist.is_empty() { track.clone() } else { format!("{artist} - {track}") };
            let fname = if self.s.music_numbers { format!("{:0width$} - {base}", k + 1) } else { base };
            let out = dir.join(format!("{}.{ext}", safe_name(&fname.replace(['/', '\\'], "_"), 150)));
            self.p.stage = "edit".into();
            self.p.item = format!("{}/{n}", k + 1);
            self.p.percent = (k as f32 / n as f32 * 100.0).min(100.0);
            self.p.speed.clear();
            self.p.eta.clear();
            self.p.message = format!("Режу на треки: {track}");
            self.emit();
            let mut a: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into(), "-ss".into(), format!("{start:.3}")];
            if let Some(end) = end {
                a.extend(["-to".into(), format!("{end:.3}")]);
            }
            a.extend(["-i".into(), audio.to_string_lossy().into_owned()]);
            if embed_cover {
                a.extend(["-i".into(), cover.to_string_lossy().into_owned()]);
            }
            a.extend(["-map".into(), "0:a:0".into()]);
            if embed_cover {
                a.extend(["-map", "1:v:0", "-c:v", "copy", "-disposition:v", "attached_pic"].map(String::from));
            }
            a.extend(codec.iter().map(|s| s.to_string()));
            for (key, val) in [
                ("title", track.as_str()),
                ("artist", artist.as_str()),
                ("album", title.as_str()),
                ("album_artist", uploader.as_str()),
                ("track", &format!("{}/{n}", k + 1)),
                ("comment", page.as_str()),
            ] {
                a.extend(["-metadata".into(), format!("{key}={val}")]);
            }
            if ext == "mp3" {
                a.extend(["-id3v2_version".into(), "3".into()]);
            }
            a.push(out.to_string_lossy().into_owned());
            let mut tail = Vec::new();
            let (ok, _) = self.spawn_lines(tool("ffmpeg"), &a, true, |_, line, _| {
                tail.push(line.to_string());
                if tail.len() > 5 {
                    tail.remove(0);
                }
            })?;
            if !ok || !out.exists() {
                self.errors += 1;
                self.log(format!("Ошибка: трек «{track}» не вырезался: {}", first_line(&tail.join(" "))));
                continue;
            }
            done += 1;
            self.log(format!("Трек: {}", out.display()));
            if self.s.source_log {
                let at = if page.contains('?') { format!("{page}&t={}s", *start as u64) } else { format!("{page}?t={}s", *start as u64) };
                note_source(&out, &format!("YouTube ({origin})"), &at, &self.asked);
            }
            if let Some(f) = out.file_name() {
                m3u.push(f.to_string_lossy().into_owned());
            }
            self.results.push(out.to_string_lossy().into_owned());
        }
        let _ = std::fs::remove_file(&audio);
        let _ = std::fs::remove_file(&thumb);
        if !m3u.is_empty() {
            if let Ok(mut f) = std::fs::File::create(dir.join(format!("{}.m3u8", safe_name(&title, 80)))) {
                let _ = write!(f, "#EXTM3U\n{}\n", m3u.join("\n"));
            }
        }
        if done == 0 {
            return fail("ни один трек не вырезался (подробности в журнале)");
        }
        self.log(format!("Готово: {done} из {n} трек(ов) в {}", dir.display()));
        Ok(())
    }
}

trait IfEmptyStr {
    fn if_empty_str(self, o: &str) -> String;
}

impl IfEmptyStr for String {
    fn if_empty_str(self, o: &str) -> String {
        if self.is_empty() { o.to_string() } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_from_description() {
        let d = "Aphex Twin - SAW II\n\nTRACKLIST\n00:00 - #1\n07:28 - #2\n1:19:44 - Blue Calx\n1:27:04 - #14\nhttps://warp.net";
        let m = parse_timestamps(d);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0], (0.0, "#1".to_string()));
        assert_eq!(m[2], (4784.0, "Blue Calx".to_string()));
        let d = "[00:00] Intro\n(03:15) 2. Artist - Song\n1) 05:40 Outro\nnot 12:5 time";
        let m = parse_timestamps(d);
        assert_eq!(m.len(), 3);
        assert_eq!(m[1].1, "Artist - Song");
        assert_eq!(m[2], (340.0, "Outro".to_string()));
        assert!(parse_timestamps("00:00 a\n01:00 b").is_empty());
        assert_eq!(split_artist("Artist - Song", "Ch"), ("Artist".into(), "Song".into()));
        assert_eq!(split_artist("#3", "Aphex Twin"), ("Aphex Twin".into(), "#3".into()));
    }
}
