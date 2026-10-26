"""Общие сведения о программе: имя, папки данных, режим собранного .exe."""
from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

APP_NAME = "Very Good Downloader"
APP_VERSION = "1.0.0"

FROZEN = getattr(sys, "frozen", False)          # запущено из .exe, собранного PyInstaller
APP_DIR = Path(sys.executable).parent if FROZEN else Path(__file__).resolve().parent
DATA_DIR = Path(os.environ.get("APPDATA", APP_DIR)) / "VeryGoodDownloader"
PYLIB_DIR = DATA_DIR / "pylib"                  # обновлённый yt-dlp для собранной версии
_OLD_DATA_DIR = Path(os.environ.get("APPDATA", APP_DIR)) / "YouTubeDownloader"


def migrate_old_data() -> None:
    """Настройки и темы из версии, которая называлась YouTube Downloader."""
    if DATA_DIR.exists() or not _OLD_DATA_DIR.exists():
        return
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    for name in ("settings.json", "topics.json"):
        if (_OLD_DATA_DIR / name).exists():
            shutil.copy2(_OLD_DATA_DIR / name, DATA_DIR / name)


def vtuple(v: str) -> tuple[int, ...]:
    """«2026.08.19» и «2026.8.19» — одна версия: сравниваем числами."""
    return tuple(int(x) for x in v.split(".") if x.isdigit())


def _version_of(pkg_dir: Path) -> str:
    try:
        text = (pkg_dir / "version.py").read_text("utf-8")
        return text.split("__version__ = '", 1)[1].split("'", 1)[0]
    except Exception:  # noqa: BLE001
        return ""


def setup_runtime() -> None:
    """Вызывается до импорта yt_dlp: PATH к ffmpeg/deno и свежий yt-dlp из PYLIB_DIR."""
    migrate_old_data()
    if not FROZEN:
        return
    bin_dir = APP_DIR / "bin"
    if bin_dir.exists():
        os.environ["PATH"] = str(bin_dir) + os.pathsep + os.environ.get("PATH", "")
    # Обновление yt-dlp кладётся в PYLIB_DIR; берём его, только если он новее встроенного.
    bundled = (APP_DIR / "yt_dlp_version.txt").read_text().strip() \
        if (APP_DIR / "yt_dlp_version.txt").exists() else ""
    updated = _version_of(PYLIB_DIR / "yt_dlp")
    if updated and vtuple(updated) > vtuple(bundled):
        sys.path.insert(0, str(PYLIB_DIR))
