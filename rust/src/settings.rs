//! Настройки: тот же settings.json, что у Python-версии (поля и значения совпадают).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use crate::appinfo::data_dir;

pub const HEIGHTS: [u32; 6] = [0, 2160, 1440, 1080, 720, 480];
pub const FPS_LIMITS: [u32; 5] = [0, 60, 30, 25, 24];
pub const EDIT_FORMATS: [(&str, &str); 3] =
    [("none", "Нет"), ("h264", "H.264 MP4 + AAC"), ("prores", "ProRes 422 HQ MOV + PCM")];
pub const BROWSERS: [&str; 9] =
    ["none", "firefox", "chrome", "edge", "brave", "opera", "vivaldi", "chromium", "file"];

pub fn edit_name(key: &str) -> &'static str {
    EDIT_FORMATS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or("Нет")
}

pub fn default_out() -> String {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    home.join("Videos").join("Very Good Downloader").to_string_lossy().into_owned()
}

/// Папка по умолчанию до версии 1.4.
fn old_default_out() -> String {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    home.join("Videos").join("YouTube").to_string_lossy().into_owned()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub out_dir: String,
    pub playlist: bool,
    pub subs: bool,
    pub sub_langs: String,
    pub auto_subs: bool,
    pub thumbnail: bool,
    /// "none" / "h264" / "prores"
    pub edit: String,
    /// раскладывать по папкам Категория/Канал
    pub sort: bool,
    /// 0 = без ограничения, иначе 2160/1440/1080/720…
    pub max_height: u32,
    /// 0 = без ограничения; если формата нет — пересчёт ffmpeg
    pub max_fps: u32,
    /// браузер из BROWSERS или "file"; включается только при необходимости
    pub cookies: String,
    pub cookies_file: String,
    /// масштаб интерфейса окна (1.0 = 100%)
    pub ui_scale: f32,
    /// размер окна при последнем закрытии (0 — по умолчанию)
    pub win_w: f32,
    pub win_h: f32,
    /// музыка: "best" (как есть), "mp3", "m4a", "flac"
    pub music_format: String,
    /// ключ приложения Spotify (developer.spotify.com) — чтобы брать плейлисты больше 100 треков
    pub spotify_id: String,
    pub spotify_secret: String,
    /// версия расширения браузера, которое уже связывалось с программой ("" — ещё ни разу)
    pub ext_version: String,
    /// вырезать спонсорские вставки (SponsorBlock)
    pub sponsorblock: bool,
    /// разбивать видео по главам на отдельные файлы
    pub split_chapters: bool,
    /// предлагать скачать скопированную ссылку
    pub clipboard_watch: bool,
    /// уведомление Windows, когда загрузка закончилась
    pub notify_done: bool,
    /// тексты песен с LRCLIB (.lrc рядом и внутри файла)
    pub lyrics: bool,
    /// подписки: каналы и плейлисты, из которых качаются только новые видео
    pub subscriptions: Vec<Subscription>,
    /// проверять подписки при запуске окна
    pub subs_on_start: bool,
    /// крестик прячет окно в трей (значок у часов), программа продолжает работать
    pub tray: bool,
    /// номер трека в имени файла: «01 - Исполнитель - Название»
    pub music_numbers: bool,
    /// недавние папки загрузок (для быстрой смены на главном экране), новые — первыми
    pub recent_dirs: Vec<String>,
    /// свёрнутые группы на странице настроек
    pub collapsed: Vec<String>,
    /// трека нет на самом сервисе (VK, SoundCloud…) — пропустить, а не искать на YouTube/SoundCloud
    pub music_no_replace: bool,
    /// треки, найденные не там, где их просили, — в подпапку «_из других мест»
    pub music_replace_dir: bool,
    /// «_откуда скачано.txt» рядом со скачанным: файл, источник, ссылка
    pub source_log: bool,
    /// копия для монтажа сохраняется вместо оригинала (MKV не остаётся)
    pub edit_replace: bool,
    /// сохранённые наборы настроек («Эдиты 1080/24», «Музыка FLAC»…)
    pub presets: Vec<Preset>,
    /// замену трека сверять по звуку с 30-секундным отрывком оригинала (Deezer)
    pub music_verify: bool,
    /// кнопки в приложении Spotify поставлены — следить, чтобы не пропадали после обновлений Spotify
    pub spotify_app: bool,
    /// искать замену в VK от аккаунта пользователя (первым, до YouTube Music)
    pub vk_search: bool,
    /// Яндекс Музыка от аккаунта: звук прямо с Яндекса и поиск замены там
    pub yandex_acc: bool,
    /// ключ API song.link (Odesli): точные ссылки на тот же трек на других сервисах
    pub odesli_key: String,
}

/// Что можно поменять пресетом или у отдельной ссылки (имена полей Settings).
pub const PRESET_KEYS: [&str; 13] = [
    "out_dir", "max_height", "max_fps", "edit", "edit_replace", "music_format", "subs", "thumbnail",
    "sponsorblock", "split_chapters", "sort", "playlist", "lyrics",
];

/// Пресет: название и значения полей из PRESET_KEYS.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preset {
    pub name: String,
    pub values: Map<String, Value>,
}

/// Подписка на канал или плейлист.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Subscription {
    pub url: String,
    pub name: String,
    pub audio_only: bool,
    /// не старше стольких дней (0 — без ограничения)
    pub max_age_days: u32,
    /// уже было запомнено, что есть на канале сейчас (старое не качаем)
    pub seeded: bool,
    /// когда проверяли последний раз (unix, секунды)
    pub last_check: u64,
}

/// Разовые параметры одной загрузки (из главного окна или консоли).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct JobOpts {
    /// фрагмент: «1:30», «90» или пусто
    pub from: String,
    pub to: String,
    /// только звук
    pub audio_only: bool,
    /// только запомнить, что сейчас есть (первая проверка подписки)
    pub seed_only: bool,
    /// пропускать видео старше стольких дней (0 — все)
    pub max_age_days: u32,
    /// пресет настроек ("" — обычные настройки)
    pub preset: String,
    /// свои настройки только для этой ссылки (поверх пресета), поля из PRESET_KEYS
    pub over: Map<String, Value>,
    /// микс или альбом одним видео → отдельные треки по главам / таймкодам
    pub split_tracks: bool,
}

impl JobOpts {
    pub fn has_section(&self) -> bool {
        !self.from.trim().is_empty() || !self.to.trim().is_empty()
    }

    /// «*1:30-2:45» для yt-dlp --download-sections
    pub fn section(&self) -> String {
        let f = if self.from.trim().is_empty() { "0" } else { self.from.trim() };
        let t = if self.to.trim().is_empty() { "inf" } else { self.to.trim() };
        format!("*{f}-{t}")
    }

    /// Такой файл не совпадает с «обычной» загрузкой видео — архив не трогаем.
    pub fn is_variant(&self) -> bool {
        self.has_section() || self.audio_only || self.split_tracks
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            out_dir: default_out(),
            playlist: true,
            subs: false,
            sub_langs: "ru,en".into(),
            auto_subs: true,
            thumbnail: true,
            edit: "none".into(),
            sort: false,
            max_height: 0,
            max_fps: 0,
            cookies: "chrome".into(),
            cookies_file: String::new(),
            ui_scale: 0.8,
            win_w: 0.0,
            win_h: 0.0,
            music_format: "best".into(),
            spotify_id: String::new(),
            spotify_secret: String::new(),
            ext_version: String::new(),
            sponsorblock: false,
            split_chapters: false,
            clipboard_watch: false,
            notify_done: true,
            lyrics: false,
            subscriptions: Vec::new(),
            subs_on_start: true,
            tray: true,
            music_numbers: true,
            recent_dirs: Vec::new(),
            collapsed: Vec::new(),
            music_no_replace: false,
            music_replace_dir: false,
            source_log: true,
            edit_replace: false,
            presets: Vec::new(),
            music_verify: true,
            spotify_app: false,
            vk_search: false,
            yandex_acc: false,
            odesli_key: String::new(),
        }
    }
}

impl Settings {
    pub fn file() -> PathBuf {
        data_dir().join("settings.json")
    }

    pub fn load() -> Self {
        let mut s: Self = std::fs::read_to_string(Self::file())
            .ok()
            // Блокнот и PowerShell 5 пишут UTF-8 с BOM — serde_json его не принимает.
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default();
        // 1.4: папка по умолчанию «Видео\YouTube» → «Видео\Very Good Downloader» (переезжает вместе с содержимым).
        if s.out_dir.eq_ignore_ascii_case(&old_default_out()) {
            let new = default_out();
            if !Path::new(&new).exists() && Path::new(&s.out_dir).exists() {
                let _ = std::fs::rename(&s.out_dir, &new);
            }
            if Path::new(&new).exists() || !Path::new(&s.out_dir).exists() {
                s.out_dir = new;
                s.save();
            }
        }
        s
    }

    pub fn save(&self) {
        let f = Self::file();
        if let Some(p) = f.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(f, t);
        }
    }

    /// Запомнить папку в недавних (до 6, без повторов).
    pub fn remember_dir(&mut self, dir: &str) {
        let dir = dir.trim().to_string();
        if dir.is_empty() {
            return;
        }
        self.recent_dirs.retain(|d| !d.eq_ignore_ascii_case(&dir));
        self.recent_dirs.insert(0, dir);
        self.recent_dirs.truncate(6);
    }

    /// Текущие значения полей, которые входят в пресет.
    pub fn preset_values(&self) -> Map<String, Value> {
        let Ok(Value::Object(all)) = serde_json::to_value(self) else { return Map::new() };
        all.into_iter().filter(|(k, _)| PRESET_KEYS.contains(&k.as_str())).collect()
    }

    /// Те же настройки, но с подменёнными полями (неизвестные поля и неверные значения пропускаются).
    pub fn with_values(&self, m: &Map<String, Value>) -> Settings {
        let Ok(Value::Object(mut all)) = serde_json::to_value(self) else { return self.clone() };
        for (k, v) in m {
            if PRESET_KEYS.contains(&k.as_str()) {
                all.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value(Value::Object(all)).unwrap_or_else(|_| self.clone())
    }

    /// Настройки для одной ссылки: общие → пресет → свои значения ссылки.
    pub fn for_job(&self, o: &JobOpts) -> Settings {
        let base = match self.presets.iter().find(|p| p.name == o.preset) {
            Some(p) if !o.preset.is_empty() => self.with_values(&p.values),
            _ => self.clone(),
        };
        if o.over.is_empty() { base } else { base.with_values(&o.over) }
    }

    pub fn sub_lang_list(&self) -> Vec<String> {
        let v: Vec<String> =
            self.sub_langs.split(',').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        if v.is_empty() {
            vec!["ru".into(), "en".into()]
        } else {
            v
        }
    }
}
