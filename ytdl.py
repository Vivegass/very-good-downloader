"""Консольная версия: ytdl <ссылка> [<ссылка> ...] [параметры].

Без параметров берутся настройки из окна программы.
"""
from __future__ import annotations

import argparse
import sys

from ytdl_core import BROWSERS, EDIT_FORMATS, Cancelled, Downloader, Progress, Settings, update_ytdlp


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8", errors="replace")
    s = Settings.load()
    ap = argparse.ArgumentParser(prog="vgd", description="Very Good Downloader: скачивание с YouTube.")
    ap.add_argument("urls", nargs="*", help="ссылки на видео, плейлисты или каналы")
    ap.add_argument("-o", "--out", default=s.out_dir, help=f"папка (сейчас: {s.out_dir})")
    ap.add_argument("-e", "--edit", choices=list(EDIT_FORMATS), default=s.edit,
                    help="копия для монтажа: none / h264 / prores")
    ap.add_argument("--no-playlist", action="store_true", help="из ссылки с плейлистом брать только само видео")
    ap.add_argument("--no-subs", action="store_true", help="не качать субтитры")
    ap.add_argument("--sub-langs", default=s.sub_langs, help="языки субтитров, например ru,en")
    ap.add_argument("--no-thumb", action="store_true", help="не сохранять обложку")
    ap.add_argument("-c", "--cookies", default=s.cookies,
                    help=f"браузер для куки ({', '.join(BROWSERS[1:-1])}), none или путь к cookies.txt")
    ap.add_argument("-r", "--max-height", type=int, default=s.max_height,
                    help="макс. разрешение по меньшей стороне: 1080, 720… (0 — без ограничения)")
    ap.add_argument("-f", "--max-fps", type=int, default=s.max_fps,
                    help="макс. fps (0 — без ограничения); если формата нет — пересчёт ffmpeg")
    ap.add_argument("--no-sort", action="store_true", help="не раскладывать по папкам Тема/Канал")
    ap.add_argument("--sort-existing", action="store_true",
                    help="разложить по папкам уже скачанные файлы в папке -o и выйти")
    ap.add_argument("-y", "--yes", action="store_true", help="не спрашивать подтверждения")
    ap.add_argument("-U", "--update", action="store_true", help="обновить yt-dlp")
    a = ap.parse_args()

    if a.update:
        update_ytdlp()
        if not a.urls:
            return 0
    if a.sort_existing:
        from ytdl_sort import apply_plan, plan_existing
        plan = plan_existing(a.out)
        for src, dst in plan:
            print(f"  {src.name}  ->  {dst.parent.relative_to(a.out)}")
        if plan and (a.yes or input(f"Переместить {len(plan)} файл(ов)? [y/N] ").lower().startswith(("y", "д"))):
            apply_plan(plan)
        return 0
    if not a.urls:
        ap.print_help()
        return 1

    s.out_dir, s.edit, s.sub_langs = a.out, a.edit, a.sub_langs
    s.max_height, s.max_fps = a.max_height, a.max_fps
    s.sort = s.sort and not a.no_sort
    s.playlist = s.playlist and not a.no_playlist
    s.subs = s.subs and not a.no_subs
    s.thumbnail = s.thumbnail and not a.no_thumb
    if a.cookies in BROWSERS:
        s.cookies = a.cookies
    else:
        s.cookies, s.cookies_file = "file", a.cookies

    last = [""]

    def show(p: Progress) -> None:
        line = f"\r{p.item + ' ' if p.item else ''}{p.message[:60]:<60} {p.percent:5.1f}%  {p.speed:>12}  {p.eta:>8}"
        if line != last[0]:
            sys.stdout.write(line)
            sys.stdout.flush()
            last[0] = line

    def log(msg: str) -> None:
        if msg.startswith("[download]") or msg.startswith("[info]"):
            return
        sys.stdout.write("\r" + " " * 100 + "\r" + msg + "\n")

    failed = 0
    for url in a.urls:
        print(f"\n▶ {url}")
        try:
            files = Downloader(s, show, log).run(url)
            print()
            for f in files:
                print(f"  ✔ {f}")
        except (KeyboardInterrupt, Cancelled):
            print("\nОтменено.")
            return 130
        except Exception as e:  # noqa: BLE001
            failed += 1
            print(f"\n  ✖ {e}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
