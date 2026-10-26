"""Прогресс фоновой загрузки: читает download_log.txt и показывает, что происходит.

Запуск: progress.cmd [папка загрузок]  (по умолчанию — папка из настроек программы).
"""
from __future__ import annotations

import os
import re
import shutil
import sys
import time
from pathlib import Path

from ytdl_core import Settings

ANSI_CLEAR = "\x1b[2J\x1b[H"


def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    os.system("")  # включает ANSI-коды в консоли Windows
    root = Path(sys.argv[1] if len(sys.argv) > 1 else Settings.load().out_dir)
    log = root / "download_log.txt"
    while True:
        try:
            text = log.read_text("utf-8", errors="replace")
        except FileNotFoundError:
            text = ""
        lines = [l.strip() for l in re.split(r"[\r\n]+", text) if l.strip()]
        done = [l for l in lines if l.startswith("Скачано:")]
        errors = [l for l in lines if l.startswith("Ошибка")]
        total = next((int(m.group(1)) for l in reversed(lines)
                      if (m := re.search(r"Downloading \d+ items of (\d+)", l))), 0)
        progress = next((l for l in reversed(lines) if re.search(r"\d+\.\d%", l)), "")
        free = shutil.disk_usage(root).free / 2**30
        try:  # архив переживает перезапуски, журнал — нет
            archived = sum(1 for l in (root / ".ytdl_archive.txt").read_text().splitlines() if l.strip())
        except FileNotFoundError:
            archived = len(done)

        out = [f"Папка: {root}", ""]
        if progress:
            m = re.match(r"(?:(\d+/\d+) )?(.*?)\s+(\d+\.\d)%\s*(\S*\s?\S*/с|\S+x)?\s*([\d:]+)?\s*$", progress)
            if m:
                item, what, pct, speed, eta = m.groups()
                bar = "█" * int(float(pct) / 2.5) + "░" * (40 - int(float(pct) / 2.5))
                out += [f"Сейчас: {what}" + (f"   (видео {item})" if item else ""),
                        f"[{bar}] {pct}%   {speed or ''}   осталось {eta or '—'}"]
            else:
                out.append("Сейчас: " + progress)
        out += ["", f"Готово видео: {archived}" + (f" из ~{total + 1}" if total else "")
                + (f"   (в этом запуске: {len(done)})" if archived != len(done) else ""),
                f"Ошибок: {len(errors)}", f"Свободно на диске: {free:.0f} ГБ", ""]
        if done:
            out.append("Последние:")
            out += ["  " + Path(l.split("  [")[0].replace("Скачано: ", "")).relative_to(root).as_posix()
                    if l.split("  [")[0].replace("Скачано: ", "").startswith(str(root)) else "  " + l
                    for l in done[-5:]]
        if errors:
            out += ["", "Последняя ошибка:", "  " + errors[-1][:200]]
        finished = any(l.startswith(("Готово:", "Новых видео нет")) for l in lines[-3:]) and not progress.endswith("%")
        out += ["", "Загрузка завершена." if finished else "Обновляется каждые 2 с · Ctrl+C — закрыть просмотр (загрузка продолжится)"]
        sys.stdout.write(ANSI_CLEAR + "\n".join(out) + "\n")
        sys.stdout.flush()
        time.sleep(2)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
