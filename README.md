# Very Good Downloader

Программа для Windows: видео (YouTube, TikTok, VK, Rutube, Pinterest…) и музыка (Spotify, Apple Music,
SoundCloud, YouTube Music, Яндекс Музыка, VK, Deezer) поверх yt-dlp и ffmpeg, с раскладкой по папкам
и расширением для браузера.

- `rust/` — программа (окно + консоль `vgd.exe`): описание, сборка и история версий — [rust/README.md](rust/README.md).
- `extension/` — расширение для Chrome / Edge / Яндекс.Браузера.

Сборка: `powershell -ExecutionPolicy Bypass -File rust\build.ps1` (нужны Rust с MSVC; Inno Setup — для установщика).
