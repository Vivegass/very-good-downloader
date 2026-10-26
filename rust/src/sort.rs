//! Сортировка видео по папкам: Тема / Канал.
//!
//! Тема определяется по ключевым словам (название, теги, канал, плейлист) из topics.json,
//! если ничего не совпало — по категории YouTube. Каналы с похожими никами
//! («STEALTH X» и «Stealth X Edits») попадают в одну папку.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::appinfo::data_dir;

pub const INDEX_NAME: &str = ".ytdl_index.json";
pub const FALLBACK_TOPIC: &str = "Разное";

pub type Topics = Vec<(String, Vec<String>)>;

/// Порядок важен: при равном счёте побеждает тема выше.
pub fn default_topics() -> Topics {
    let t: &[(&str, &[&str])] = &[
        ("Эдиты", &["edit", "edits", "эдит", "эдиты", "amv", "mmv", "montage", "монтаж", "velocity", "twixtor",
                   "quickedit", "flow edit", "trickshot", "trickshotting", "clips in desc", "edit contest"]),
        ("Туториалы", &["tutorial", "туториал", "урок", "how to", "как сделать", "guide", "гайд", "курс",
                       "breakdown", "разбор"]),
        ("Моушн и VFX", &["motion design", "моушн", "vfx", "cgi", "after effects", "blender", "houdini",
                         "cinema 4d", "c4d", "3d animation", "render", "embergen"]),
        ("Игры", &["gameplay", "геймплей", "прохождение", "let's play", "speedrun", "beamng", "cs2",
                  "counter-strike", "minecraft", "gta", "valorant", "fortnite", "assetto corsa", "iracing",
                  "dota", "rust"]),
        ("Музыка", &["official music video", "music video", "official audio", "клип", "remix", "lyrics",
                    "phonk", "album", "альбом", "концерт", "live performance"]),
        ("Кино", &["trailer", "трейлер", "movie", "фильм", "film", "scene", "сцена", "сериал"]),
    ];
    t.iter().map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect())).collect()
}

const CATEGORY_TOPICS: &[(&str, &str)] = &[
    ("Gaming", "Игры"), ("Music", "Музыка"), ("Film & Animation", "Кино"), ("Education", "Туториалы"),
    ("Howto & Style", "Туториалы"), ("Science & Technology", "Технологии"), ("Autos & Vehicles", "Авто"),
    ("Sports", "Спорт"), ("Comedy", "Юмор"), ("Entertainment", "Развлечения"), ("People & Blogs", "Блоги"),
    ("News & Politics", "Новости"), ("Travel & Events", "Путешествия"), ("Pets & Animals", "Животные"),
];

/// Слова, которые не отличают один канал от другого: «X Edits» = «X» = «X Official».
const NICK_NOISE: &[&str] = &[
    "official", "officiel", "channel", "tv", "yt", "youtube", "edit", "edits", "clips", "clip", "shorts",
    "music", "gaming", "games", "hd", "4k", "studio", "studios", "archive", "backup", "vevo", "topic",
    "канал", "официальный", "эдиты", "эдит", "клипы", "the",
];

pub fn topics_file() -> PathBuf {
    data_dir().join("topics.json")
}

fn topics_to_json(t: &Topics) -> String {
    let mut m = Map::new();
    for (k, v) in t {
        m.insert(k.clone(), json!(v));
    }
    serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default()
}

pub fn ensure_topics_file() -> PathBuf {
    let f = topics_file();
    if !f.exists() {
        if let Some(p) = f.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = std::fs::write(&f, topics_to_json(&default_topics()));
    }
    f
}

pub fn load_topics() -> Topics {
    let f = topics_file();
    match std::fs::read_to_string(&f) {
        Ok(text) => {
            if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
                let t: Topics = m
                    .into_iter()
                    .map(|(k, v)| {
                        let words = v
                            .as_array()
                            .map(|a| a.iter().filter_map(|w| w.as_str()).map(|w| w.to_lowercase()).collect())
                            .unwrap_or_default();
                        (k, words)
                    })
                    .collect();
                if !t.is_empty() {
                    return t;
                }
            }
            default_topics() // битый файл: работаем со стандартными темами
        }
        Err(_) => {
            ensure_topics_file();
            default_topics()
        }
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Сколько раз `kw` встречается в `text` целым словом (как `(?<!\w)kw(?!\w)` в Python).
fn count_word(text: &str, kw: &str) -> usize {
    if kw.is_empty() {
        return 0;
    }
    let mut n = 0;
    let mut start = 0;
    while let Some(pos) = text[start..].find(kw) {
        let i = start + pos;
        let end = i + kw.len();
        let before_ok = text[..i].chars().next_back().map_or(true, |c| !is_word(c));
        let after_ok = text[end..].chars().next().map_or(true, |c| !is_word(c));
        if before_ok && after_ok {
            n += 1;
            start = end;
        } else {
            start = i + text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    n
}

fn s(info: &Value, key: &str) -> String {
    info.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn first_str(info: &Value, keys: &[&str]) -> String {
    keys.iter().map(|k| s(info, k)).find(|v| !v.is_empty()).unwrap_or_default()
}

pub fn detect_topic(info: &Value, topics: &Topics) -> String {
    let title = s(info, "title").to_lowercase();
    let tags = info
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" | "))
        .unwrap_or_default()
        .to_lowercase();
    let chan = first_str(info, &["channel", "uploader"]).to_lowercase();
    // Плейлист «Very Good Edits» — сильная подсказка для всех его видео.
    let pl = first_str(info, &["playlist_title", "playlist"]).to_lowercase();
    let mut best: Option<&str> = None;
    let mut best_score = 0;
    for (topic, words) in topics {
        let score: usize = words
            .iter()
            .map(|w| {
                count_word(&title, w) * 3 + count_word(&tags, w).min(3) + count_word(&chan, w) * 2
                    + count_word(&pl, w) * 4
            })
            .sum();
        if score > best_score {
            best = Some(topic);
            best_score = score;
        }
    }
    if let Some(b) = best {
        return b.to_string();
    }
    if let Some(cats) = info.get("categories").and_then(Value::as_array) {
        for c in cats.iter().filter_map(Value::as_str) {
            if let Some((_, t)) = CATEGORY_TOPICS.iter().find(|(k, _)| *k == c) {
                return t.to_string();
            }
        }
    }
    // Не YouTube (Pinterest, TikTok, VK…): тема — название сайта, а не «Разное».
    site_name(info).unwrap_or_else(|| FALLBACK_TOPIC.to_string())
}

/// «Pinterest», «TikTok», «VK»… для видео не с YouTube.
pub fn site_name(info: &Value) -> Option<String> {
    let ex = s(info, "extractor_key");
    if ex.is_empty() || ex.starts_with("Youtube") || ex == "Generic" && s(info, "webpage_url_domain").is_empty() {
        return None;
    }
    const KNOWN: &[(&str, &str)] = &[
        ("pinterest", "Pinterest"), ("tiktok", "TikTok"), ("instagram", "Instagram"), ("vk.com", "VK"),
        ("vkvideo", "VK"), ("twitter", "X (Twitter)"), ("x.com", "X (Twitter)"), ("reddit", "Reddit"),
        ("vimeo", "Vimeo"), ("rutube", "Rutube"), ("twitch", "Twitch"), ("dailymotion", "Dailymotion"),
        ("facebook", "Facebook"), ("ok.ru", "Одноклассники"), ("bilibili", "Bilibili"), ("soundcloud", "SoundCloud"),
        ("coub", "Coub"), ("dzen", "Дзен"), ("streamable", "Streamable"), ("kick", "Kick"),
    ];
    let domain = s(info, "webpage_url_domain").to_lowercase();
    let key = ex.to_lowercase();
    let hit = |k: &str| {
        if k.contains('.') {
            domain == k || domain.ends_with(&format!(".{k}"))
        } else {
            domain.contains(k) || key.starts_with(k)
        }
    };
    if let Some((_, n)) = KNOWN.iter().find(|(k, _)| hit(k)) {
        return Some(n.to_string());
    }
    let d = domain.trim_start_matches("www.").trim_start_matches("m.");
    Some(if d.is_empty() { ex } else { d.to_string() })
}

/// Имя папки без запрещённых в Windows символов.
pub fn safe_name(name: &str, limit: usize) -> String {
    let cleaned: String =
        name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '_' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    let cut: String = trimmed.chars().take(limit).collect();
    let cut = cut.trim_end_matches(['.', ' ']).to_string();
    if cut.is_empty() {
        "Без названия".into()
    } else {
        cut
    }
}

fn nick_tokens(name: &str) -> Vec<String> {
    let lower = name.to_lowercase().replace('ё', "е");
    let tokens: Vec<String> =
        lower.split(|c: char| !is_word(c)).filter(|t| !t.is_empty()).map(str::to_string).collect();
    let core: Vec<String> = tokens.iter().filter(|t| !NICK_NOISE.contains(&t.as_str())).cloned().collect();
    if core.is_empty() {
        tokens
    } else {
        core
    }
}

/// difflib.SequenceMatcher(None, a, b).ratio() для коротких строк (без эвристики autojunk).
pub fn seq_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    fn matches(a: &[char], b: &[char]) -> usize {
        // Самый длинный общий кусок (первый по a, затем по b), потом рекурсивно слева и справа.
        let (mut bi, mut bj, mut best) = (0, 0, 0);
        let mut prev = vec![0usize; b.len() + 1];
        for i in 0..a.len() {
            let mut cur = vec![0usize; b.len() + 1];
            for j in 0..b.len() {
                if a[i] == b[j] {
                    cur[j + 1] = prev[j] + 1;
                    let k = cur[j + 1];
                    if k > best {
                        best = k;
                        bi = i + 1 - k;
                        bj = j + 1 - k;
                    }
                }
            }
            prev = cur;
        }
        if best == 0 {
            return 0;
        }
        best + matches(&a[..bi], &b[..bj]) + matches(&a[bi + best..], &b[bj + best..])
    }
    2.0 * matches(&a, &b) as f64 / total as f64
}

pub fn similar_nicks(a: &str, b: &str) -> bool {
    let (ta, tb) = (nick_tokens(a), nick_tokens(b));
    let (ka, kb) = (ta.concat(), tb.concat());
    if ka.is_empty() || kb.is_empty() {
        return false;
    }
    if ka == kb {
        return true;
    }
    // «troubled duck» и «troubled duck and plugdi»: короткий ник — начало длинного целыми словами.
    let (short, long) = if ta.len() <= tb.len() { (&ta, &tb) } else { (&tb, &ta) };
    if short.concat().chars().count() >= 4 && long.len() >= short.len() && long[..short.len()] == short[..] {
        return true;
    }
    // Опечатки и вариации написания длинных ников.
    ka.chars().count().min(kb.chars().count()) >= 6 && seq_ratio(&ka, &kb) >= 0.9
}

/// Хранит соответствие канал → папка в .ytdl_index.json внутри папки загрузок.
pub struct Sorter {
    root: PathBuf,
    index: Map<String, Value>,
    topics: Topics,
}

impl Sorter {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let mut index = std::fs::read_to_string(root.join(INDEX_NAME))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if !index.get("channels").is_some_and(Value::is_object) {
            index.insert("channels".into(), Value::Object(Map::new()));
        }
        Self { root, index, topics: load_topics() }
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(&self.root);
        if let Ok(t) = serde_json::to_string_pretty(&self.index) {
            let _ = std::fs::write(self.root.join(INDEX_NAME), t);
        }
    }

    fn channels(&mut self) -> &mut Map<String, Value> {
        self.index.get_mut("channels").and_then(Value::as_object_mut).expect("channels")
    }

    pub fn channel_folder(&mut self, info: &Value) -> String {
        let cid = first_str(info, &["channel_id", "uploader_id"]);
        let mut name = first_str(info, &["channel", "uploader", "creator", "uploader_id"]);
        if name.is_empty() {
            name = "Неизвестный канал".into();
        }
        if !cid.is_empty() {
            if let Some(f) = self.channels().get(&cid).and_then(Value::as_str) {
                return f.to_string();
            }
        }
        let mut known: Vec<String> = Vec::new();
        for v in self.channels().values().filter_map(Value::as_str) {
            if !known.iter().any(|k| k == v) {
                known.push(v.to_string());
            }
        }
        let mut folder = known.into_iter().find(|f| similar_nicks(&name, f));
        if folder.is_none() {
            // Папки, созданные раньше вручную или без индекса, тоже учитываем.
            let existing = existing_channel_dirs(&self.root);
            folder = existing.into_iter().find(|f| similar_nicks(&name, f));
        }
        let folder = folder.unwrap_or_else(|| safe_name(&name, 80));
        if !cid.is_empty() {
            self.channels().insert(cid, Value::String(folder.clone()));
            self.save();
        }
        folder
    }

    /// (тема, папка канала)
    pub fn target_dir(&mut self, info: &Value) -> (String, String) {
        let topic = safe_name(&detect_topic(info, &self.topics), 80);
        (topic, self.channel_folder(info))
    }
}

fn existing_channel_dirs(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(topics) = std::fs::read_dir(root) else { return out };
    for t in topics.flatten().filter(|e| e.path().is_dir()) {
        if let Ok(chans) = std::fs::read_dir(t.path()) {
            for c in chans.flatten().filter(|e| e.path().is_dir()) {
                let n = c.file_name().to_string_lossy().into_owned();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// `[id]` из 11 символов в имени файла.
pub fn video_id_in_name(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut i = 0;
    while let Some(p) = name[i..].find('[') {
        let start = i + p + 1;
        let end = start + 11;
        if bytes.get(end) == Some(&b']') {
            if let Some(id) = name.get(start..end) {
                if id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                    return Some(id.to_string());
                }
            }
        }
        i = start;
    }
    None
}

/// Файлы с `[id]` в корне папки, сгруппированные по id (в порядке появления).
pub fn group_root_files(root: &Path) -> Vec<(String, Vec<PathBuf>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut entries: Vec<_> = rd.flatten().filter(|e| e.path().is_file()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if let Some(id) = video_id_in_name(&e.file_name().to_string_lossy()) {
                if !groups.contains_key(&id) {
                    order.push(id.clone());
                }
                groups.entry(id).or_default().push(e.path());
            }
        }
    }
    order.into_iter().map(|id| {
        let files = groups.remove(&id).unwrap_or_default();
        (id, files)
    }).collect()
}

/// Переместить файлы по плану. Возвращает число перемещённых.
pub fn apply_plan(plan: &[(PathBuf, PathBuf)], log: &dyn Fn(String)) -> usize {
    let mut moved = 0;
    for (src, dst) in plan {
        if !src.exists() || dst.exists() {
            continue;
        }
        if let Some(p) = dst.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        match std::fs::rename(src, dst) {
            Ok(()) => moved += 1,
            Err(_) => {
                // Другой диск: копируем и удаляем.
                if std::fs::copy(src, dst).is_ok() && std::fs::remove_file(src).is_ok() {
                    moved += 1;
                }
            }
        }
    }
    log(format!("Перемещено файлов: {moved}"));
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nicks() {
        assert!(similar_nicks("STEALTH X", "Stealth X Edits"));
        assert!(similar_nicks("troubled duck", "troubled duck and plugdi"));
        assert!(!similar_nicks("abc", "xyz"));
        assert!(similar_nicks("Velocityedits", "Velocityedit"));
    }

    #[test]
    fn ratio_matches_difflib() {
        // difflib.SequenceMatcher(None, "abcd", "bcde").ratio() == 0.75
        assert!((seq_ratio("abcd", "bcde") - 0.75).abs() < 1e-9);
        assert!((seq_ratio("velocityedits", "velocityedit") - 24.0 / 25.0).abs() < 1e-9);
    }

    #[test]
    fn words() {
        assert_eq!(count_word("my edit | edits", "edit"), 1);
        assert_eq!(count_word("эдит эдиты", "эдит"), 1);
        assert_eq!(count_word("let's play gta", "let's play"), 1);
    }

    #[test]
    fn topic() {
        let t = default_topics();
        let info = json!({"title": "STATER edit", "categories": ["Gaming"]});
        assert_eq!(detect_topic(&info, &t), "Эдиты");
        let info = json!({"title": "something", "categories": ["Gaming"]});
        assert_eq!(detect_topic(&info, &t), "Игры");
        let info = json!({"title": "x", "playlist_title": "Very Good Edits"});
        assert_eq!(detect_topic(&info, &t), "Эдиты");
        let info = json!({"title": "cat", "extractor_key": "Pinterest", "webpage_url_domain": "pinterest.com"});
        assert_eq!(detect_topic(&info, &t), "Pinterest");
        let info = json!({"title": "cat", "extractor_key": "Generic", "webpage_url_domain": "dropbox.com"});
        assert_eq!(detect_topic(&info, &t), "dropbox.com");
        let info = json!({"title": "cat", "extractor_key": "Youtube"});
        assert_eq!(detect_topic(&info, &t), "Разное");
    }

    #[test]
    fn ids() {
        assert_eq!(video_id_in_name("ALKAREN [vnFS2RaAeGc].mkv").as_deref(), Some("vnFS2RaAeGc"));
        assert_eq!(video_id_in_name("[x] a [jNQXAC9IVRw]_edit.mp4").as_deref(), Some("jNQXAC9IVRw"));
        assert_eq!(video_id_in_name("no id.mkv"), None);
    }
}
