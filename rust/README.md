# Very Good Downloader

Окно и консоль на Rust. Сам YouTube качает **yt-dlp.exe** (аналога на Rust, который успевал бы за YouTube, нет),
ffmpeg делает копии для монтажа, deno нужен yt-dlp для всех форматов.

| Файл | Что это |
|---|---|
| `Very Good Downloader.exe` (`vgd-gui`) | окно, egui/glow, ~4,6 МБ |
| `vgd.exe` | консоль, ~0,6 МБ, `vgd --help` — все ключи, `--watch` — прогресс фоновой загрузки |
| `bin\yt-dlp.exe`, `bin\ffmpeg.exe`, `bin\deno.exe` | внешние программы; ffprobe больше не нужен |

Настройки и темы: `%APPDATA%\VeryGoodDownloader\settings.json`, `topics.json`.
Архив `.ytdl_archive.txt` и индекс каналов `.ytdl_index.json` — в папке загрузок.

## Сборка

```powershell
powershell -ExecutionPolicy Bypass -File rust\build.ps1
```

Собирает `cargo build --release`, кладёт всё в `rust\dist\VeryGoodDownloader\` и, если установлен
Inno Setup 6, делает `release\VeryGoodDownloader-Setup-<версия>.exe`. Установщик ставится поверх
старых версий (тот же AppId) и удаляет остатки версии 1.0 (`_internal`).

Для разработки: положить `yt-dlp.exe`, `ffmpeg.exe`, `deno.exe` в `rust\bin\` — программа из
`target\release` найдёт их там. Тесты: `cargo test --lib`.

## Как устроено (src/)

- `downloader.rs` — по каждому видео: `yt-dlp -j` → тема/канал считаются в Rust и вписываются
  в сведения → `yt-dlp --load-info-json` по шаблону `%(sort_topic)s/%(sort_channel)s/…` →
  пересчёт fps, субтитры, копия для монтажа → запись в архив (только после всей обработки).
  Плейлист/канал сначала разворачивается через `--flat-playlist`, уже скачанное пропускается по архиву.
- `net.rs` — проверка прокси/VPN (HTTP(S)_PROXY или системный прокси Windows). Если порт не отвечает —
  понятное сообщение и пауза, пока он не вернётся; видео, упавшее из-за прокси, повторяется.
- `sort.rs` — темы и похожие ники (сравнение как у Python difflib.SequenceMatcher).
- `media.rs` — сведения о файле из `ffmpeg -i` (без ffprobe).
- `watch.rs` — `vgd --watch [папка]`: прогресс фоновой загрузки по `download_log.txt`.

Устойчивость загрузки:
- проверка VPN/прокси перед стартом и перед каждым видео;
- видео с HTTP 403 повторяются в конце через 30 с;
- автосубтитры только на языке оригинала (`xx-orig`): автоперевод почти всегда давал 429;
- если куки браузера не читаются, программа один раз говорит об этом и дальше качает без них.

## Расширение для браузера (1.2.0)

Папка `extension\` (в установке — `{app}\extension`): Manifest V3 для Chrome / Edge / Яндекс.Браузера.
Ставится через «Загрузить распакованное» на странице расширений в режиме разработчика.

- На YouTube под видео кнопка «скачать» (на Shorts, плейлистах и каналах — плавающая кнопка справа внизу).
  Ссылка чистится: без «микса» `list=RD…`, времени и меток.
- Значок расширения и пункт в меню правой кнопки — для любого сайта (Pinterest, VK, Rutube, TikTok…).
- `src/ipc.rs`: окно слушает `127.0.0.1:38517` (`POST /add`, нужен заголовок `X-VGD: 1`, Origin только
  `chrome-extension://`). Если окно закрыто, расширение открывает `vgd://add?url=…`. Протокол регистрирует
  установщик (галочка «Запускать программу из расширения браузера») или кнопка «включить» в настройках.
  Второй запуск программы не открывает второе окно, а передаёт ссылки первому.
- Не YouTube: тема — название сайта (Pinterest, VK, Rutube…), если ключевые слова не подошли.
  Проверено: Pinterest, VK, Rutube качаются; Vimeo и Reddit требуют вход (куки браузера);
  TikTok может отвечать «IP заблокирован» в зависимости от VPN.

## Музыка (1.3.0)

Ссылки Spotify, SoundCloud, YouTube Music, Яндекс Музыки и VK идут в музыкальную загрузку
(`src/music.rs` — списки треков, `src/downloader/music_dl.rs` — скачивание).

- SoundCloud, YouTube Music — звук прямо с сервиса (yt-dlp).
- Spotify — список со встраиваемой страницы (до 100 треков) или через Web API с ключом
  пользователя (`spotify_id` / `spotify_secret` в настройках, страницы по 100 — без ограничения).
- Яндекс Музыка — открытый API (трек, альбом, артист, плейлист), постранично, без ограничения.
- VK — список треков со страницы плейлиста передаёт расширение (`extension/vk.js`), в программу —
  `POST /add {name, service, tracks}` → файл в `%APPDATA%\VeryGoodDownloader\lists` → ссылка `vgdlist:`.
- Звук для Spotify/Яндекса/VK и замена недоступным трекам: YouTube Music → SoundCloud → YouTube,
  проверка по длительности (±5%, не меньше 8 с) и словам названия, без каверов/live/ускоренных.
- Файлы: `<папка>\Музыка\<плейлист>\NN - Исполнитель - Название.ext`, теги и квадратная обложка
  внутри, `<плейлист>.m3u8`, архив `Музыка\.music_archive.txt` (повтор пропускает скачанное).
- Формат: «как есть» (opus/m4a без перекодирования), mp3 320, m4a, flac.
- Консоль: `vgd --limit N` (первые N треков), `--music-format mp3`.

## 1.7

- Поиск звука: YouTube Music → SoundCloud → **Audius** (api.audius.co, полный mp3 320) → YouTube;
  если у трека нет ISRC — он берётся из открытого API **Deezer** и ищется на YouTube Music первым.
  Для проверки одного источника: переменная `VGD_ONLY_SOURCE=audius|ytmusic|soundcloud|youtube`.
- Тексты песен **LRCLIB**: в тег файла (через `meta_lyrics` yt-dlp) и `.lrc` рядом.
- Параметры загрузки (`JobOpts`): фрагмент «с/до» (`--download-sections` + `--force-keyframes-at-cuts`),
  «только звук»; не пишутся в архив. Консоль: `--section 1:30-2:45`, `--audio`.
- Настройки: SponsorBlock (`--sponsorblock-remove sponsor,selfpromo,interaction`), главы (`--split-chapters`),
  буфер обмена, уведомления Windows (тост через PowerShell, щелчок открывает папку).
- Подписки (`settings.subscriptions`): первая проверка запоминает всё, что есть (`seed_only`), дальше качаются
  только новые; «не старше N дней»; «только звук». Вкладка «подписки» и `vgd sync` для Планировщика.
- TikTok: при равном разрешении предпочитается H.264 (1080p у TikTok часто только в HEVC — тогда HEVC).

## 1.8

- Дискография исполнителя: каталог Deezer (`deezer_discography`) — альбомы, EP, синглы, по папкам «год - альбом».
  Работает для ссылок исполнителя Spotify / Apple Music / Deezer и кнопки «Дискография» в VK.
- Без повторов: треки, уже лежащие в «Музыка» (по «исполнитель - название» в имени файла), не качаются ни с какого сервиса.
- 4 трека параллельно (`Downloader::fork`), без пауз между треками; отмена гасит все процессы сразу.
- Трей (`tray-icon`): свёрнутое окно прячется в значок, загрузки идут; щелчок — открыть, меню — «Выход».
- Расширение 1.8: VK — ↓ в ряду значков трека, белая ↓ в ряду шапки с меню (плейлист / обложка / список .txt),
  «Дискография» у исполнителя; плейлист собирается только из своего блока. Яндекс — после «…» и перед ♥.
  Spotify — ↓ в плеере и «дискография» у исполнителя.

## 1.9

- **VK — звук прямо из VK**, как у VK Music Saver, без поиска на YouTube/SoundCloud.
  `extension/vk-main.js` (контекст страницы, от имени вошедшего пользователя) по запросу `vk.js` берёт ссылки
  на звук: `vkApi.api("audio.getById")` пачками по 100, для старой разметки — `al_audio.php?act=reload_audio`
  (по хешам из `data-audio`), ссылки с `audio_api_unavailable` раскодирует. Трек уходит в программу с полем
  `url` (HLS `.m3u8`, сегменты AES-128) и обложкой альбома; `music.rs::read_list` кладёт его в `Track.source`.
- Дальше всё как у SoundCloud: `yt-dlp -j` (generic-экстрактор открывает .m3u8 и расшифровывает) →
  `music_download` — те же папки `Музыка\<плейлист>\NN - Исполнитель - Название`, теги, обложка, архив,
  формат из настроек («как есть» = mp3 320 из VK без перекодирования). В журнале «… — взял из VK.».
- Плейлист целиком берётся через API (`audio.getAudioIdsBySource` → `audio.getById`, запасной — `audio.get`),
  название — `audio.getPlaylistById`; id плейлиста — из свойств React, ссылок в шапке или `z=audio_playlist…`.
  Строки со страницы — только если API не ответил. Трек в новой разметке — `props.track.entity.apiAudio`.
- Расширение кладёт в список поле `diag` (что нашлось на странице) — `%APPDATA%\VeryGoodDownloader\lists\*.json`.
- Ответ VK 5xx / таймаут — два повтора (через 3 и 8 с), потом поиск замены.
- Нет ссылки (трек заблокирован, нет прав) или она протухла, пока ждала в очереди, — замена ищется как раньше.
- Проверено: расшифровка ссылок (тест на круговое кодирование), загрузка зашифрованного HLS-потока тем же
  yt-dlp и теми же ключами, `cargo test`. **Не проверено на живой странице VK** — нужен вход в аккаунт.

## 1.10

- Расширение для Firefox (подпись Mozilla, `rust/firefox.ps1`), установщик предлагает поставить расширение.
- Редактор категорий в окне вместо ручной правки `topics.json`, смена папки на главном экране,
  сворачивание групп настроек, отключаемая нумерация треков.

## 1.11

- **Очередь** (`vgd-gui.rs`): состояние `Paused`; пауза = отмена текущего процесса с пометкой в `App::pausing`,
  «продолжить» ставит ссылку обратно — скачанное пропускается по архиву, недокачанный `.part` yt-dlp продолжает.
  Порядок — стрелки ↑↓; очередь рабочих нитей (`Shared::queue`) пересобирается из карточек в `sync_queue`.
  Две нити: основная и `side` — берёт только небольшую музыку (`music::is_small_music`: трек, альбом,
  список из браузера до 20 треков), пока основная занята.
- **Пресеты** (`Settings::presets`, поля из `PRESET_KEYS`) и свои настройки ссылки (`JobOpts::preset`, `JobOpts::over`);
  настройки для загрузки — `Settings::for_job`: общие → пресет → свои. Консоль: `--preset <название>`.
- **Микс → треки** (`downloader/mix.rs`, `JobOpts::split_tracks`, консоль `--split-tracks`): главы из `yt-dlp -j`,
  иначе таймкоды в описании (`parse_timestamps`), иначе в комментариях (закреплённый — первым).
  Звук качается целиком один раз, ffmpeg режет: «как есть» — без перекодирования (opus/m4a), иначе mp3/m4a/flac;
  теги (альбом = название видео, номер, «Исполнитель - Название» из главы), обложка внутри и `cover.jpg`, `.m3u8`.
- **Проверка по звуку** (`fingerprint.rs`, `Settings::music_verify`, консоль `--no-verify`): отрывок 30 с с Deezer
  (`music::deezer_preview`) сравнивается с найденной заменой — корреляция лог-спектрограмм (24 полосы 200–4000 Гц)
  в лучшем сдвиге. Проверено: тот же трек 94%, sped up 6%, другая песня 13%; порог `SAME_MIN` = 50%.
  Не совпало — файл удаляется, поиск идёт дальше.
- Музыка: «искать замену недоступным трекам» (`music_no_replace`, `--no-substitutes`) и «замены — в отдельную папку»
  (`music_replace_dir`, подпапка `_из других мест`) — только для треков, у которых есть свой звук (VK, SoundCloud,
  YouTube Music); для Spotify/Apple/Яндекса поиск — единственный путь.
- `_откуда скачано.txt` (`note_source`, настройка `source_log`): дата, файл, сайт, ссылка, что просили скачать.
- «Вместо оригинала» (`edit_replace`, консоль `--edit-only`): один проход ffmpeg сразу в H.264/ProRes (с пределом fps),
  MKV удаляется.
- Окно: запасные шрифты (Yu Gothic, Microsoft YaHei, Malgun Gothic, Segoe UI Symbol) — японские, китайские и корейские
  названия и знаки вроде 【】 больше не квадратики; «все загрузки завершены» не висит, пока очередь идёт.

## Приложение Spotify (1.11)

- `spicetify/vgd.js` — расширение Spicetify (вшито в программу, `spotify_app::EXTENSION_JS`). Меню правой кнопки
  (`Spicetify.ContextMenu`), кнопка в шапке (`action-bar-row`), кнопка в плеере (`Spicetify.Playbar.Button`).
  Списки — внутри клиента: плейлист `GraphQL fetchPlaylistContents` по 50 → `Platform.PlaylistAPI.getContents` →
  Web API через `CosmosAsync`; «Любимые» — `LibraryAPI.getTracks` → `/v1/me/tracks`; альбом — Web API; исполнитель —
  ссылкой (дискографию собирает программа). Отправка — `POST /add` со списком (`service: "Spotify"`, ключи
  `spotify:<id>` как у ссылок — архив общий). Программа закрыта — `vgd://add` и ожидание `/ping`.
- `ipc.rs`: Origin `https://xpui.app.spotify.com` разрешён, OPTIONS и ответы — с заголовками CORS (тест `spotify_cors`).
- `spotify_app.rs`: установка — свежий Spicetify с GitHub (zip → `%LOCALAPPDATA%\spicetify`, распаковка `System32\tar.exe`:
  GNU tar из Git считает «C:» сетевым адресом), `vgd.js` в `%APPDATA%\spicetify\Extensions`, `config spotify_path/prefs_path/
  extensions`, `backup apply` (или `apply`, если уже применён). Чужая резервная копия (другая папка Spotify или Spotify
  обновился) откладывается в `Backup-<версия>`, запись о ней в `config-xpui.ini` сбрасывается.
- «Не слетает»: признак — `Apps\xpui.spa` вернулся (Spotify обновился). Проверка при запуске окна и при входе в Windows
  (`HKCU\…\Run\VeryGoodDownloader-Spotify` → `Very Good Downloader.exe --spotify-repair`, без окна); чинит только пока
  Spotify закрыт (`apply --no-restart`).
- Подсмотрено у grademach/spotify-downloader: постраничный `fetchPlaylistContents` и данные текущего трека из
  `Player.data.item`. abbas-rz/Spicetify-Spotify-Downloader — перехват родной кнопки Download + spotDL через свой
  Python-сервер (у нас сервер — сама программа, ставить ничего не нужно).

## Поиск в VK от аккаунта пользователя (1.11)

- `vk_web.rs` + `vk_agent.js`: «невидимый браузер» — тот же exe окна в режиме `--vk-agent` (tao + wry, WebView2)
  открывает vk.com/audio скрыто; папка данных `%APPDATA%\VeryGoodDownloader\webview2` общая с окном входа
  `--vk-login` (пользователь входит сам на vk.com, пароль программе не виден). Связь — JSON-строки: stdin `{rid,q,count}`,
  stdout `{rid,items|error}` и `{type:"state",logged,api}`. В странице — `vkApi.api("audio.search")` и расшифровка
  `audio_api_unavailable` (код из extension/vk-main.js). Процесс запускается при первом поиске и живёт, пока жива программа
  (закрыли stdin — вышел); неудачный запуск (нет входа) запоминается на 5 минут.
- Настройка `vk_search`: VK — первым в поиске замены (кроме треков, которые и так из VK); проверка по звуку — как у всех.
- Проверено: «Shiro SAGISU — MISATO» (не было на YouTube Music/SoundCloud/Audius) — взят с VK, звук совпал на 96%.
- Музыка: архив скачанного — свой у каждой папки; трек, уже лежащий в другой папке «Музыка», копируется, а не качается.
- Окно: крестик при включённом трее прячет программу к часам; «свернуть» — как обычно.

## Яндекс Музыка от аккаунта и song.link (1.11)

- `yandex_acc.rs` (по образцу MarshalX/yandex-music-api): вход OAuth Device Flow (`oauth.yandex.ru/device/code` →
  пользователь вводит код на ya.ru/device → `/token`), токен в `%APPDATA%\VeryGoodDownloader\yandex_token.json`.
  Файл — `get-file-info` с подписью HMAC-SHA256 (ключ и `X-Yandex-Music-Client` десктопного приложения), только
  `transports=raw`; `encraw` (зашифрованный) не берём и не расшифровываем. «как есть»/flac → `quality=lossless`, иначе `hq`.
  Консоль: `vgd yandex`. Настройка `yandex_acc`.
- Трек Яндекса — сначала прямо с Яндекса; для Spotify/Apple/… Яндекс ищется после VK (`/search?type=track`).
  Списки Яндекса тоже читаются с токеном: без него API отвечает «Unavailable For Legal Reasons» (VPN/страна).
- Проверено: Daft Punk — Harder, Better, Faster, Stronger с Яндекса, aac-mp4, полный трек 3:46.
- song.link (Odesli): `music::odesli` — точные ссылки на тот же трек (YouTube Music → SoundCloud → YouTube) перед поиском.
  С осени 2026 открытый API отвечает `PUBLIC_API_ACCESS_DEPRECATED` — работает только с ключом (`odesli_key`);
  без ключа или после отказа не вызывается.
- Не делаем: расшифровку `encraw`, торренты/Soulseek (Jackett, Gazelle, slskd) — раздача чужой музыки.

## После 1.11 (правки от пользователей)

- **Имена видео без `[id]`**: скачивается по-прежнему под именем с `[id]` (докачка, уникальность), затем
  `strip_id_from_names` переименовывает файл, обложку и папку глав. Одинаковые названия разных видео — «Название (2)»;
  то же видео повторно — заменяется. id пишется в тег «комментарий» файла (`[id] ссылка`, `meta_comment` +
  `--embed-metadata`; ffmpeg переносит его в копию для монтажа) и читается `media::probe` → `video_id`.
  «Разложить папку» берёт id из тега, у старых файлов — из имени. Уже скачанные файлы не переименовываются.
- Журнал: поиск и Ctrl+F. Уведомления Windows — от имени программы (`HKCU\Software\Classes\AppUserModelId\VeryGoodDownloader.App`).
- Spotify: `vgd.js` не запускается дважды (`window.__vgdLoaded`); устаревший `vgd.js` в Spotify обновляется при запуске окна.
- Расширение: значок фиксированного размера; после перезагрузки расширения «осиротевшие» кнопки на открытых вкладках убираются.

## Темы оформления

- `theme.rs` — тема: 28 цветов (`#rrggbb`), `radius` (множитель скругления: 0 — квадратные углы, 1 — как в тёмной),
  `font` (`mono` / `sans` = Segoe UI), `icons` (`line` = Segoe MDL2 / `symbols` = обычные знаки ↓ ⚙ ☰, крупнее)
  и `layout` — раскладка и форма, задаётся только в файле:
  `nav` (`left`/`top`/`bottom`), `nav_labels`, `nav_width` (от 140 — пункты строками), `header` (`logo`/`name`/`none`),
  `column`, `cards` (`fill`/`outline`/`none`), `shadow` (`none`/`shadow`/`glow`), `density`, `padding`,
  `buttons` (`pill`/`rect`/`outline`/`flat`), `background` (градиент из 2–3 цветов), `background_image` (PNG из папки тем)
  + `image_opacity`, `font_scale`, `bold_headings`, `caps` (`lower`/`upper` — подписи разделов).
- Встроенные: тёмная (по умолчанию, прежний вид), светлая, «закат», «терминал» — меняют цвета и форму;
  «неон» (навигация сверху, рамки со свечением, градиент, контурные кнопки), «облако» (навигация снизу значками,
  тени, плоские кнопки, Segoe UI), «бумага» (широкая левая панель, квадратные углы, прописные жирные заголовки).
- Свои темы — `%APPDATA%\VeryGoodDownloader\themes\*.json`, одна тема = один файл, можно с комментариями `//` и `/* */`.
  Недостающие поля (и весь `layout`) берутся из тёмной — старые файлы читаются. Кнопка «папка тем» кладёт туда
  `_пример — все поля.json` (`src/theme_example.json`) с пояснением к каждому полю. Выбранная — `Settings::theme`
  (`""` — тёмная, `light`/`sunset`/`terminal`/`neon`/`cloud`/`paper`, `file:<имя файла>` — своя).
- В окне цвета берутся из `pal()` (thread_local), скругления — `rad()`/`btn_rad()`, отступы — `pad()`/`margin()`
  (растут вместе со скруглением), высоты — `dens()`, значки — `glyph()`. `apply_theme()` применяет тему сразу;
  шрифты и картинка фона пересобираются, только если сменились.
- Настройки → «оформление» (`bin/gui/appearance.rs`): выбор темы, «создать» (копия текущей), пример кнопок,
  пипетка для каждого цвета, ползунок скругления, шрифт и значки, «сохранить» название, удалить, папка тем, импорт/экспорт.

### Material 3 (и перенос на Android)

- Встроенные «material 3» и «material 3 светлая» (`material`, `material-light`) собраны из seed `#6750a4` функцией
  `theme::from_m3`. Форма задаётся общими параметрами `layout`: `buttons: "tonal"` (главная — filled, обычные — tonal,
  маленькие — outlined), `button_radius: 100` (full), `field_radius: 4` (ExtraSmall), карточки — скругление 12 (Medium),
  `nav_indicator: "pill"` (Navigation Rail: «таблетка» вокруг значка, подпись под ней), `nav_width: 80`, `switch: "m3"`,
  `font: "roboto"` (Roboto Regular, заголовки — Medium; без Roboto — Segoe UI / Semibold).
- Своя тема на M3 — достаточно `{"name": "…", "m3": {"seed": "#00639b", "dark": false}}`: роли выводятся из seed
  (`theme/m3.rs`, «tonal spot»: primary — оттенок seed, secondary — C16, tertiary — оттенок +60° C24, neutral C4,
  neutral variant C8, error — от #b3261e; тона ролей — по спецификации M3). Тон — L* (как в HCT), оттенок и насыщенность —
  CIELAB LCh вместо CAM16, поэтому цвета близки к Material Theme Builder, но не точь-в-точь (тест: базовая схема в пределах
  ±28 по каналу). Всё, что задано в файле явно, остаётся как задано.
- При сохранении в файле появляется `m3.scheme` — все роли с именами полей Compose `ColorScheme` (camelCase:
  `primary`, `onPrimary`, `primaryContainer`, `surfaceContainerLow`…). Android читает `m3.scheme` и строит
  `ColorScheme(...)` / `darkColorScheme(...)` напрямую, без своего пересчёта; `m3.dark` — какая из двух.
- Соответствие полей окна ролям M3 (`from_m3`):

  | поле темы | роль M3 | поле темы | роль M3 |
  |---|---|---|---|
  | `bg`, `sidebar`, `sidebar_line` | `surface` | `text`, `text_hover` | `onSurface` |
  | `surface` (карточки) | `surfaceContainer` | `muted` | `onSurfaceVariant` |
  | `surface_hi`, `field` | `surfaceContainerHighest` | `faint`, `border`, `knob_off` | `outline` |
  | `window` | `surfaceContainerHigh` | `divider`, `outline` | `outlineVariant` |
  | `pill` (tonal-кнопки, индикатор навигации), `track` | `secondaryContainer` | `selection` | `primaryContainer` |
  | `accent` | `primary` | `on_accent`, `knob_on` | `onPrimary` |
  | `accent_hover` | `primary` + 8% `onPrimary` (state layer) | `toggle_off` | `surfaceContainerHighest` |
  | `err` | `error` | `ok`/`ok_bg`, `warn`/`warn_bg` | доп. цвета (как custom colors): зелёный #386a20 и янтарный #7c5800, тон 40/90 (светлая) или 80/30 (тёмная) |

### Инструкция «КАК СДЕЛАТЬ ТЕМУ.html»

- Подробная инструкция с настоящими скриншотами: `assets/theme-guide/` (HTML + `инструкция/*.png`, ~1.7 МБ).
  Встроена в exe (`theme/guide.rs`, список — `theme/guide_files.rs`) и кладётся в папку тем при запуске окна и по
  кнопке «папка тем», если её там нет или `guide::VERSION` новее (`инструкция/версия.txt`). Файлы тем пользователя не трогаются.
  Установщику ничего добавлять не нужно. Пересборка скриншотов и текста — `tools/theme-guide/README.md`.
- Файл выбранной своей темы перечитывается сам раз в секунду после сохранения (правка в Блокноте видна сразу);
  файлы с ошибкой не пропадают молча — под списком тем пишется, какой файл и в какой строке.
