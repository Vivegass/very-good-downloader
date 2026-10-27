# Very Good Downloader — кнопки «скачать» в Spotify

![Кнопки в Spotify](https://raw.githubusercontent.com/Vivegass/very-good-downloader/main/docs/spicetify.png)

Расширение [Spicetify](https://spicetify.app) для программы [Very Good Downloader](https://github.com/Vivegass/very-good-downloader) (Windows).
Добавляет в приложение Spotify кнопки, которые отправляют музыку в очередь программы:

- **меню правой кнопки** — «Скачать в Very Good Downloader» у трека, нескольких выделенных треков, плейлиста,
  альбома, исполнителя (дискография) и «Любимых треков»;
- **кнопка ↓ в шапке** плейлиста и альбома — рядом с большой Play;
- **кнопка в плеере** — трек, который играет сейчас.

Плейлисты — **любой длины**: список треков собирается внутри Spotify, без ограничения «первые 100».
Скачивает сама программа: звук ищется на VK, Яндекс Музыке (от вашего аккаунта), YouTube Music, SoundCloud, Audius
и YouTube и сверяется по звуку с оригиналом; теги, обложки, папка плейлиста.

## Установка

**Проще всего — из программы:** «Настройки → приложение spotify → установить в spotify» (или `vgd spotify`).
Программа сама поставит или обновит Spicetify (совместим со SpotX), включит расширение и будет возвращать кнопки,
когда Spotify обновится.

**Через Marketplace:** найдите «Very Good Downloader» во вкладке Marketplace и нажмите Install.
Программа Very Good Downloader должна быть установлена — без неё кнопкам некуда отправлять.

**Вручную:** положите `vgd.js` в папку расширений Spicetify и выполните
`spicetify config extensions vgd.js` и `spicetify apply`.

Spotify из Microsoft Store Spicetify не поддерживает.

---

**English.** Spicetify extension for the Very Good Downloader app (Windows): adds “Download in Very Good Downloader”
to the context menu (track, selection, playlist, album, artist, Liked Songs), a download button in the playlist
header and in the player bar. Playlists of any length. Requires the Very Good Downloader app running locally
(it listens on `127.0.0.1:38517`).
