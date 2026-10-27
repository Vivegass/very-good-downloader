//! Консольная версия: vgd <ссылка> [<ссылка> ...] [параметры].
//! Без параметров берутся настройки из окна программы.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use vgd::appinfo::{APP_NAME, APP_VERSION};
use vgd::downloader::{plan_existing, update_ytdlp, DlError, Downloader, Progress};
use vgd::settings::{Settings, BROWSERS, EDIT_FORMATS};
use vgd::sort::apply_plan;

fn help(s: &Settings) -> String {
    format!(
        "{APP_NAME} {APP_VERSION}: скачивание с YouTube.

Использование: vgd [параметры] <ссылка> [<ссылка> ...]

  -o, --out <папка>        папка (сейчас: {out})
  -e, --edit <формат>      копия для монтажа: none / h264 / prores (сейчас: {edit})
      --edit-only          сохранить сразу в формате -e, без оригинала MKV
      --preset <название>  пресет настроек из окна программы (остальные ключи — поверх него)
  -r, --max-height <N>     макс. разрешение по меньшей стороне: 1080, 720… (0 — без ограничения)
  -f, --max-fps <N>        макс. fps (0 — без ограничения); если формата нет — пересчёт ffmpeg
  -c, --cookies <браузер>  куки для 18+/закрытых: {browsers}, none или путь к cookies.txt
      --sub-langs <языки>  языки субтитров, например ru,en
      --no-playlist        из ссылки с плейлистом брать только само видео
      --no-subs            не качать субтитры
      --no-thumb           не сохранять обложку
      --no-sort            не раскладывать по папкам Тема/Канал
      --sort-existing      разложить по папкам уже скачанные файлы в папке -o и выйти
  -y, --yes                не спрашивать подтверждения
  -U, --update             обновить yt-dlp
      --limit <N>          из плейлиста брать только первые N треков/видео
      --section <с-до>     только фрагмент, например 1:30-2:45 (или 90-165)
      --audio              только звук
      --split-tracks       микс/альбом одним видео → отдельные треки по главам или таймкодам
      --sponsorblock       вырезать спонсорские вставки
      --chapters           разбить видео по главам на отдельные файлы

  vgd yandex               войти в Яндекс Музыку (кодом) — звук прямо с Яндекса и поиск там
  vgd spotify              поставить кнопки «скачать» в приложение Spotify (Spicetify; совместимо со SpotX)
  vgd sync                 проверить подписки (из окна программы) и скачать новые видео —
                           удобно для Планировщика заданий Windows
      --music-format <ф>   музыка: best (как есть) / mp3 / m4a / flac
      --no-substitutes     не искать замену трекам, недоступным на самом сервисе (VK, SoundCloud…)
      --no-verify          не сверять найденную замену по звуку с отрывком оригинала (Deezer)
      --watch [папка]      показывать прогресс фоновой загрузки (по download_log.txt)
  -h, --help               эта справка
",
        out = s.out_dir,
        edit = s.edit,
        browsers = BROWSERS[1..BROWSERS.len() - 1].join(", ")
    )
}

struct Args {
    urls: Vec<String>,
    out: Option<String>,
    edit: Option<String>,
    max_height: Option<u32>,
    max_fps: Option<u32>,
    cookies: Option<String>,
    sub_langs: Option<String>,
    no_playlist: bool,
    no_subs: bool,
    edit_only: bool,
    no_verify: bool,
    split_tracks: bool,
    preset: Option<String>,
    no_substitutes: bool,
    no_thumb: bool,
    no_sort: bool,
    sort_existing: bool,
    yes: bool,
    update: bool,
    watch: Option<Option<String>>,
    limit: Option<usize>,
    music_format: Option<String>,
    section: Option<String>,
    audio: bool,
    sponsorblock: bool,
    chapters: bool,
    sync: bool,
    spotify: bool,
    yandex: bool,
    help: bool,
}

fn parse() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;
    let mut a = Args {
        urls: vec![], out: None, edit: None, max_height: None, max_fps: None, cookies: None, sub_langs: None,
        no_playlist: false, no_subs: false, edit_only: false, no_verify: false, split_tracks: false, preset: None, no_substitutes: false, no_thumb: false, no_sort: false, sort_existing: false, yes: false,
        update: false, watch: None, help: false, limit: None, music_format: None, section: None, audio: false,
        sponsorblock: false, chapters: false, sync: false, spotify: false, yandex: false,
    };
    let mut p = lexopt::Parser::from_env();
    while let Some(arg) = p.next()? {
        match arg {
            Short('o') | Long("out") => a.out = Some(p.value()?.string()?),
            Short('e') | Long("edit") => {
                let v = p.value()?.string()?;
                if !EDIT_FORMATS.iter().any(|(k, _)| *k == v) {
                    return Err(format!("--edit: ожидается none, h264 или prores, а не «{v}»").into());
                }
                a.edit = Some(v);
            }
            Short('r') | Long("max-height") => a.max_height = Some(p.value()?.parse()?),
            Short('f') | Long("max-fps") => a.max_fps = Some(p.value()?.parse()?),
            Short('c') | Long("cookies") => a.cookies = Some(p.value()?.string()?),
            Long("sub-langs") => a.sub_langs = Some(p.value()?.string()?),
            Long("no-playlist") => a.no_playlist = true,
            Long("no-subs") => a.no_subs = true,
            Long("edit-only") => a.edit_only = true,
            Long("split-tracks") => a.split_tracks = true,
            Long("no-verify") => a.no_verify = true,
            Long("preset") => a.preset = Some(p.value()?.string()?),
            Long("no-substitutes") => a.no_substitutes = true,
            Long("no-thumb") => a.no_thumb = true,
            Long("no-sort") => a.no_sort = true,
            Long("sort-existing") => a.sort_existing = true,
            Short('y') | Long("yes") => a.yes = true,
            Short('U') | Long("update") => a.update = true,
            Long("watch") => a.watch = Some(p.optional_value().map(|v| v.string()).transpose()?),
            Short('h') | Long("help") => a.help = true,
            Long("limit") => a.limit = Some(p.value()?.parse()?),
            Long("section") => a.section = Some(p.value()?.string()?),
            Long("audio") => a.audio = true,
            Long("sponsorblock") => a.sponsorblock = true,
            Long("chapters") => a.chapters = true,
            Long("music-format") => a.music_format = Some(p.value()?.string()?),
            Value(v) if v == "sync" && a.urls.is_empty() => a.sync = true,
            Value(v) if v == "spotify" && a.urls.is_empty() => a.spotify = true,
            Value(v) if v == "yandex" && a.urls.is_empty() => a.yandex = true,
            Value(v) => {
                let v = v.string()?;
                // «--watch G:\папка» без «=»: следующий аргумент — папка, а не ссылка.
                if matches!(a.watch, Some(None)) && !v.contains("://") {
                    a.watch = Some(Some(v));
                } else {
                    a.urls.push(v);
                }
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(a)
}

fn main() -> ExitCode {
    let mut s = Settings::load();
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("vgd: {e}\nСправка: vgd --help");
            return ExitCode::from(2);
        }
    };
    if a.yandex {
        return match vgd::yandex_acc::login(&|code, url| {
            println!("Откройте {url} и введите код: {code}");
            vgd::appinfo::open_url(url);
        }) {
            Ok(()) => {
                let mut s2 = s.clone();
                s2.yandex_acc = true;
                s2.save();
                println!("Вход в Яндекс Музыку выполнен.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("vgd: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if a.spotify {
        let mut s2 = s.clone();
        return match vgd::spotify_app::install(true, &|m| println!("{m}")) {
            Ok(()) => {
                s2.spotify_app = true;
                s2.save();
                if let Ok(gui) = std::env::current_exe().map(|e| e.with_file_name("Very Good Downloader.exe")) {
                    if gui.exists() {
                        vgd::spotify_app::set_autorepair(true, &gui);
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("vgd: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if a.sync {
        return sync(&mut s);
    }
    if let Some(name) = &a.preset {
        match s.presets.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => s = s.with_values(&p.values),
            None => {
                let names: Vec<&str> = s.presets.iter().map(|p| p.name.as_str()).collect();
                eprintln!("vgd: нет пресета «{name}». Есть: {}", if names.is_empty() { "—".into() } else { names.join(", ") });
                return ExitCode::from(2);
            }
        }
    }
    if a.help {
        print!("{}", help(&s));
        return ExitCode::SUCCESS;
    }
    if let Some(dir) = &a.watch {
        let root = PathBuf::from(dir.clone().or(a.out.clone()).unwrap_or(s.out_dir.clone()));
        vgd::watch::watch(&root);
    }
    if a.update {
        if let Err(e) = update_ytdlp(&|m| println!("{m}")) {
            eprintln!("Не удалось обновить yt-dlp: {e}");
            return ExitCode::FAILURE;
        }
        if a.urls.is_empty() {
            return ExitCode::SUCCESS;
        }
    }
    if let Some(o) = a.out {
        s.out_dir = o;
    }
    if a.sort_existing {
        return sort_existing(&s.out_dir, a.yes, a.cookies.as_deref());
    }
    if a.urls.is_empty() {
        print!("{}", help(&s));
        return ExitCode::from(1);
    }

    if let Some(e) = a.edit {
        s.edit = e;
    }
    if let Some(f) = a.music_format {
        s.music_format = f;
    }
    if let Some(l) = a.sub_langs {
        s.sub_langs = l;
    }
    if let Some(h) = a.max_height {
        s.max_height = h;
    }
    if let Some(f) = a.max_fps {
        s.max_fps = f;
    }
    s.sort = s.sort && !a.no_sort;
    s.sponsorblock |= a.sponsorblock;
    s.split_chapters |= a.chapters;
    s.edit_replace |= a.edit_only;
    s.music_no_replace |= a.no_substitutes;
    s.music_verify &= !a.no_verify;
    s.playlist = s.playlist && !a.no_playlist;
    s.subs = s.subs && !a.no_subs;
    s.thumbnail = s.thumbnail && !a.no_thumb;
    if let Some(c) = a.cookies {
        if BROWSERS.contains(&c.as_str()) {
            s.cookies = c;
        } else {
            s.cookies = "file".into();
            s.cookies_file = c;
        }
    }

    let last = Arc::new(Mutex::new(String::new()));
    let show = {
        let last = last.clone();
        Arc::new(move |p: &Progress| {
            let msg: String = p.message.chars().take(60).collect();
            let item = if p.item.is_empty() { String::new() } else { format!("{} ", p.item) };
            let line = format!("\r{item}{msg:<60} {:5.1}%  {:>12}  {:>8}", p.percent, p.speed, p.eta);
            let mut l = last.lock().unwrap();
            if *l != line {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(line.as_bytes());
                let _ = out.flush();
                *l = line;
            }
        })
    };
    let log = Arc::new(|m: String| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "\r{}\r{m}", " ".repeat(100));
        let _ = out.flush();
    });

    // Ctrl+C: дочерние yt-dlp/ffmpeg получают его сами; мы просто выходим с кодом 130.
    let mut failed = 0;
    for url in &a.urls {
        println!("\n▶ {url}");
        let mut dl = Downloader::new(s.clone(), show.clone(), log.clone());
        dl.limit = a.limit;
        if let Some(sec) = &a.section {
            let (f, t) = sec.split_once('-').unwrap_or((sec.as_str(), ""));
            dl.opts.from = f.trim().to_string();
            dl.opts.to = t.trim().to_string();
        }
        dl.opts.audio_only = a.audio;
        dl.opts.split_tracks = a.split_tracks;
        match dl.run(url) {
            Ok(files) => {
                println!();
                for f in files {
                    println!("  ✔ {f}");
                }
            }
            Err(DlError::Cancelled(m)) => {
                println!("\n{}", if m.is_empty() { "Отменено.".to_string() } else { m });
                return ExitCode::from(130);
            }
            Err(DlError::Failed(m)) => {
                failed += 1;
                println!("\n  ✖ {m}");
            }
        }
    }
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Проверка подписок без окна (для Планировщика заданий Windows).
fn sync(s: &mut Settings) -> ExitCode {
    if s.subscriptions.is_empty() {
        println!("Подписок нет — добавьте их в окне программы, вкладка «подписки».");
        return ExitCode::SUCCESS;
    }
    let log = Arc::new(|m: String| println!("{m}"));
    let show = Arc::new(|_: &Progress| {});
    let mut failed = 0;
    let subs = s.subscriptions.clone();
    for (i, sub) in subs.iter().enumerate() {
        println!("\n▶ {}", if sub.name.is_empty() { &sub.url } else { &sub.name });
        let mut dl = Downloader::new(s.clone(), show.clone(), log.clone());
        dl.opts.audio_only = sub.audio_only;
        dl.opts.max_age_days = sub.max_age_days;
        dl.opts.seed_only = !sub.seeded;
        match dl.run(&sub.url) {
            Ok(_) => {
                s.subscriptions[i].seeded = true;
                s.subscriptions[i].last_check = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
            }
            Err(e) => {
                failed += 1;
                println!("  ✖ {e}");
            }
        }
    }
    s.save();
    if failed > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn sort_existing(root: &str, yes: bool, _cookies: Option<&str>) -> ExitCode {
    let root_p = PathBuf::from(root);
    let plan = match plan_existing(&root_p, &|m| println!("{m}"), None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    for (src, dst) in &plan {
        let rel = dst.parent().and_then(|p| p.strip_prefix(&root_p).ok()).map(|p| p.display().to_string()).unwrap_or_default();
        println!("  {}  ->  {rel}", src.file_name().unwrap_or_default().to_string_lossy());
    }
    if plan.is_empty() {
        println!("В корне папки нет неразложенных видео с [id] в имени.");
        return ExitCode::SUCCESS;
    }
    let go = yes || {
        print!("Переместить {} файл(ов)? [y/N] ", plan.len());
        let _ = std::io::stdout().flush();
        let mut ans = String::new();
        let _ = std::io::stdin().read_line(&mut ans);
        let a = ans.trim().to_lowercase();
        a.starts_with('y') || a.starts_with('д')
    };
    if go {
        apply_plan(&plan, &|m| println!("{m}"));
    }
    ExitCode::SUCCESS
}
