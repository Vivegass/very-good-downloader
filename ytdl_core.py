"""Ядро загрузчика: yt-dlp (максимальное качество) + копия для монтажа через ffmpeg.

Используется и окном (ytdl_gui.py), и консолью (ytdl.py).
"""
from __future__ import annotations

import glob
import json
import re
import os
import shutil
import subprocess
import sys
from collections import deque
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Callable

from appinfo import APP_DIR, DATA_DIR, FROZEN, PYLIB_DIR, setup_runtime, vtuple

setup_runtime()  # до импорта yt_dlp: PATH к ffmpeg/deno, обновлённый yt-dlp
SETTINGS_FILE = DATA_DIR / "settings.json"
DEFAULT_OUT = Path.home() / "Videos" / "YouTube"

# Папки, куда winget кладёт ffmpeg и deno (на случай, если PATH процесса устарел).
_WINGET = Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft" / "WinGet"


def _ensure_tools_on_path() -> None:
    """yt-dlp ищет ffmpeg и deno через PATH; добавляем их, если не видны (режим разработки)."""
    need = [t for t in ("ffmpeg", "deno") if not shutil.which(t)]
    if not need:
        return
    extra = []
    for root in (_WINGET / "Links", _WINGET / "Packages"):
        if not root.exists():
            continue
        for tool in need:
            for exe in root.rglob(f"{tool}.exe"):
                extra.append(str(exe.parent))
                break
    if extra:
        os.environ["PATH"] = os.pathsep.join(dict.fromkeys(extra)) + os.pathsep + os.environ.get("PATH", "")


_ensure_tools_on_path()

import yt_dlp  # noqa: E402  (после настройки PATH)

from ytdl_sort import Sorter  # noqa: E402

NO_WINDOW = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0

HEIGHTS = [0, 2160, 1440, 1080, 720, 480]
FPS_LIMITS = [0, 60, 30, 25, 24]

EDIT_FORMATS = {
    "none": "Нет",
    "h264": "H.264 MP4 + AAC",
    "prores": "ProRes 422 HQ MOV + PCM",
}
VIDEO_TMPL = "%(title).150B [%(id)s].%(ext)s"
# Плейлист/канал → своя папка, файлы пронумерованы по порядку.
PLAYLIST_TMPL = "%(playlist_title,playlist_id)s/%(playlist_index)03d - " + VIDEO_TMPL
# Сортировка: Тема/Канал (поля вписывает pre_process-постпроцессор из ytdl_sort).
SORTED_TMPL = "%(sort_topic)s/%(sort_channel)s/" + VIDEO_TMPL
# Ошибки, при которых стоит повторить с куки браузера.
_NEEDS_LOGIN = re.compile(r"sign in|log ?in|age[- ]restrict|inappropriate|members|join this channel|"
                          r"private video|not a bot|premium", re.I)

BROWSERS = ["none", "firefox", "chrome", "edge", "brave", "opera", "vivaldi", "chromium", "file"]


@dataclass
class Settings:
    out_dir: str = str(DEFAULT_OUT)
    playlist: bool = True
    subs: bool = True
    sub_langs: str = "ru,en"
    auto_subs: bool = True
    thumbnail: bool = True
    edit: str = "h264"
    sort: bool = True             # раскладывать по папкам Тема/Канал
    max_height: int = 0           # 0 = без ограничения, иначе 2160/1440/1080/720…
    max_fps: int = 0              # 0 = без ограничения; если формата нет — пересчёт ffmpeg
    cookies: str = "firefox"     # браузер из BROWSERS или "file"; включается только при необходимости
    cookies_file: str = ""

    @classmethod
    def load(cls) -> "Settings":
        try:
            data = json.loads(SETTINGS_FILE.read_text("utf-8"))
            return cls(**{k: v for k, v in data.items() if k in cls.__dataclass_fields__})
        except Exception:
            return cls()

    def save(self) -> None:
        SETTINGS_FILE.parent.mkdir(parents=True, exist_ok=True)
        SETTINGS_FILE.write_text(json.dumps(asdict(self), ensure_ascii=False, indent=2), "utf-8")


@dataclass
class Progress:
    stage: str = ""              # "video", "audio", "merge", "edit", "done", "error"
    title: str = ""
    percent: float = 0.0
    speed: str = ""
    eta: str = ""
    item: str = ""               # "3/25" для плейлистов
    message: str = ""
    files: list[str] = field(default_factory=list)


class Cancelled(KeyboardInterrupt):
    """Наследник KeyboardInterrupt: yt-dlp не глушит его даже при ignoreerrors."""


MIN_FREE_GB = 15


def _fmt_bytes(n: float | None) -> str:
    if not n:
        return ""
    for unit in ("Б", "КБ", "МБ", "ГБ"):
        if n < 1024:
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} ТБ"


def _fmt_eta(s: float | None) -> str:
    if s is None:
        return ""
    s = int(s)
    h, rem = divmod(s, 3600)
    m, sec = divmod(rem, 60)
    return f"{h}:{m:02d}:{sec:02d}" if h else f"{m}:{sec:02d}"


class Downloader:
    """Скачивает один URL (видео, плейлист или канал)."""

    def __init__(self, settings: Settings, on_progress: Callable[[Progress], None] | None = None,
                 log: Callable[[str], None] | None = None):
        self.s = settings
        self.on_progress = on_progress or (lambda p: None)
        self.log = log or (lambda m: None)
        self.cancelled = False
        self._ffmpeg: subprocess.Popen | None = None
        self._p = Progress()
        self._logger = _Logger(self.log)
        self.results: list[str] = []

    def cancel(self) -> None:
        self.cancelled = True
        if self._ffmpeg and self._ffmpeg.poll() is None:
            self._ffmpeg.kill()

    # ---------- yt-dlp ----------
    def _ydl_opts(self, use_cookies: bool = False) -> dict:
        s = self.s
        out = Path(s.out_dir)
        tmpl = SORTED_TMPL if s.sort else VIDEO_TMPL
        opts: dict = {
            # Лучшее видео + лучший звук; сортировка yt-dlp по умолчанию уже ставит
            # выше разрешение, затем fps, HDR и более современный кодек.
            "format": "bestvideo*+bestaudio/best",
            "merge_output_format": "mkv",
            "paths": {"home": str(out), "temp": str(out / ".parts")},
            # pl_thumbnail пустой — обложку самого плейлиста не сохранять (у неё нет темы/канала).
            "outtmpl": {"default": tmpl, "pl_thumbnail": ""},
            "noplaylist": not s.playlist,
            "ignoreerrors": "only_download" if s.playlist else False,
            "windowsfilenames": True,
            "continuedl": True,
            "retries": 20,
            "fragment_retries": 50,
            "concurrent_fragment_downloads": 4,
            "noprogress": True,
            "quiet": True,
            "no_warnings": False,
            "progress_hooks": [self._hook],
            "postprocessor_hooks": [self._pp_hook],
            "logger": self._logger,
            "postprocessors": [],
            # Архив: при повторном запуске уже скачанные видео пропускаются.
            "download_archive": str(out / ".ytdl_archive.txt"),
            "sleep_interval": 2, "max_sleep_interval": 6,
            "sleep_interval_requests": 0.5,
        }
        # Ограничения качества — через сортировку: лучшее из того, что не выше лимита.
        # res = меньшая сторона кадра, так что вертикальные 1080x1920 тоже считаются 1080p.
        sort = []
        if s.max_height:
            sort.append(f"res:{s.max_height}")
        if s.max_fps:
            sort.append(f"fps:{s.max_fps}")
        if sort:
            opts["format_sort"] = sort
        # Субтитры качаются отдельным шагом (_download_subs): их сбой не должен ронять видео.
        if s.thumbnail:
            opts["writethumbnail"] = True
            opts["postprocessors"].append({"key": "FFmpegThumbnailsConvertor", "format": "jpg"})
        if use_cookies:
            opts.update(self._cookie_opts())
        return opts

    def _cookie_opts(self) -> dict:
        s = self.s
        if s.cookies == "file" and s.cookies_file:
            return {"cookiefile": s.cookies_file}
        if s.cookies not in ("none", "file", ""):
            try:
                from yt_dlp.cookies import extract_cookies_from_browser
                extract_cookies_from_browser(s.cookies)
                return {"cookiesfrombrowser": (s.cookies,)}
            except Exception as e:  # noqa: BLE001
                self.log(f"Предупреждение: не удалось взять куки из {s.cookies} ({e}).")
        return {}

    def _sort_pp(self):
        """pre_process-постпроцессор: до выбора имени файла вписывает тему и папку канала."""
        sorter = Sorter(self.s.out_dir)

        class SortPP(yt_dlp.postprocessor.PostProcessor):
            def run(self, info):
                info["sort_topic"], info["sort_channel"] = sorter.target_dir(info)
                return [], info
        return SortPP()

    def _emit(self, **kw) -> None:
        for k, v in kw.items():
            setattr(self._p, k, v)
        self.on_progress(self._p)

    def _hook(self, d: dict) -> None:
        if self.cancelled:
            raise Cancelled()
        info = d.get("info_dict") or {}
        title = info.get("title") or self._p.title
        idx, n = info.get("playlist_index"), info.get("n_entries") or info.get("playlist_count")
        item = f"{idx}/{n}" if idx and n else ""
        kind = "audio" if info.get("vcodec") == "none" else "video"
        if d["status"] == "downloading":
            total = d.get("total_bytes") or d.get("total_bytes_estimate")
            pct = (d.get("downloaded_bytes", 0) / total * 100) if total else 0.0
            self._emit(stage=kind, title=title, item=item, percent=pct,
                       speed=(_fmt_bytes(d.get("speed")) + "/с") if d.get("speed") else "",
                       eta=_fmt_eta(d.get("eta")),
                       message=f"{'Звук' if kind == 'audio' else 'Видео'} "
                               f"{info.get('format_note') or info.get('format_id', '')} · "
                               f"{_fmt_bytes(total)}")
        elif d["status"] == "finished":
            self._emit(percent=100.0, speed="", eta="")

    def _pp_hook(self, d: dict) -> None:
        if self.cancelled:
            raise Cancelled()
        if d["status"] == "started" and d.get("postprocessor") == "Merger":
            self._emit(stage="merge", percent=0.0, message="Склеиваю видео и звук…")

    # ---------- основной запуск ----------
    def run(self, url: str) -> list[str]:
        """Возвращает список итоговых файлов (оригиналы и копии для монтажа)."""
        self._p = Progress(title=url, message="Получаю информацию…")
        self.on_progress(self._p)
        self.results = []
        self._logger.errors = 0
        _check_disk(self.s.out_dir)
        # Без куки YouTube отдаёт больше форматов (у вошедших аккаунтов бывает урезанный
        # SABR-режим), поэтому куки включаются только если без них видео недоступно.
        try:
            info = self._download(url, use_cookies=False)
        except yt_dlp.utils.DownloadError as e:
            if not _NEEDS_LOGIN.search(str(e)) or not self._cookie_opts():
                raise
            self.log("Видео требует входа в аккаунт — повторяю с куки браузера.")
            info = self._download(url, use_cookies=True)

        if not self.results and self._logger.errors:
            raise RuntimeError("Ни одно видео не скачалось (подробности в журнале).")
        n = len(list(_iter_videos(info)))
        msg = (f"Готово: {len(self.results)} файл(ов)" if self.results
               else "Новых видео нет — всё уже скачано раньше.")
        if self._logger.errors:
            msg += f", ошибок: {self._logger.errors} (см. журнал)"
        self.log(msg + (f" · видео в ссылке: {n}" if n > 1 else ""))
        self._emit(stage="done", percent=100.0, speed="", eta="", files=list(self.results), message=msg)
        return list(self.results)

    def _download(self, url: str, use_cookies: bool) -> dict:
        with yt_dlp.YoutubeDL(self._ydl_opts(use_cookies)) as ydl:
            if self.s.sort:
                ydl.add_post_processor(self._sort_pp(), when="pre_process")
            ydl.add_post_processor(_FinishPP(self), when="after_move")
            # Сначала без обработки форматов узнаём, что это: видео или плейлист/канал.
            info = ydl.extract_info(url, download=False, process=False)
            if (info and not self.s.sort and self.s.playlist
                    and info.get("_type") in ("playlist", "multi_video")):
                ydl.params["outtmpl"]["default"] = PLAYLIST_TMPL
            info = ydl.process_ie_result(info, download=True) if info else None
        if info is None:
            raise RuntimeError("Не удалось получить видео (подробности в журнале).")
        return info

    def finish_video(self, info: dict, path: str) -> None:
        """Сразу после скачивания: пересчёт fps, субтитры, копия для монтажа."""
        idx, n = info.get("playlist_index"), info.get("n_entries") or info.get("playlist_count")
        self._p.item = f"{idx}/{n}" if idx and n else ""
        self.log(f"Скачано: {path}  [{_describe(info)}]")
        fps = _video_fps(path)
        edit = self.s.edit
        if self.s.max_fps and fps > self.s.max_fps + 0.01:
            # Формата с нужной частотой на YouTube не было — пересчитываем. Результат сразу
            # H.264 MP4 (открывается в AE), поэтому отдельная H.264-копия уже не нужна.
            # Если MP4 уже есть (прошлый запуск прервался), перезаписываем — имя уникально по [id].
            new = Path(path).with_suffix(".mp4")
            self.make_edit_copy(path, info, fmt="h264", fps=self.s.max_fps, dst=new,
                                label=f"{fps:g} -> {self.s.max_fps} fps")
            Path(path).unlink(missing_ok=True)
            path = str(new)
            if edit == "h264":
                edit = "none"
        self.results.append(path)
        if self.s.subs:
            self._download_subs(info, path)
        if edit != "none":
            self.results.append(self.make_edit_copy(path, info, fmt=edit))
        _check_disk(self.s.out_dir)

    def _download_subs(self, info: dict, video_path: str) -> None:
        """Субтитры рядом с видео. Ошибки (например, 429 у автоперевода) только в журнал."""
        p = Path(video_path)
        langs = [l.strip() for l in self.s.sub_langs.split(",") if l.strip()] or ["ru", "en"]
        opts = {
            "skip_download": True,
            "writesubtitles": True,
            "writeautomaticsub": self.s.auto_subs,
            "subtitleslangs": langs,
            "paths": {"home": str(p.parent)},
            "outtmpl": {"default": p.stem.replace("%", "%%") + ".%(ext)s"},
            "sleep_interval_subtitles": 2,
            "ignoreerrors": True,
            "quiet": True,
            "logger": _Logger(lambda m: self.log(m) if "Ошибка" in m or "429" in m else None),
        }
        self._emit(stage="subs", percent=0.0, speed="", eta="", message="Субтитры…")
        before = set(p.parent.glob(glob.escape(p.stem) + ".*.srt"))
        try:
            with yt_dlp.YoutubeDL(opts) as ydl:
                ydl.extract_info(info.get("webpage_url") or info["id"], download=True)
        except Exception as e:  # noqa: BLE001
            self.log(f"Субтитры не скачались: {str(e).splitlines()[0][:200]}")
        # С skip_download конвертер yt-dlp не запускается — переводим .vtt в .srt сами.
        for vtt in p.parent.glob(glob.escape(p.stem) + ".*.vtt"):
            srt = vtt.with_suffix(".srt")
            r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(vtt), str(srt)],
                               capture_output=True, creationflags=NO_WINDOW)
            if r.returncode == 0 and srt.exists():
                vtt.unlink(missing_ok=True)
        got = set(p.parent.glob(glob.escape(p.stem) + ".*.srt")) - before
        if got:
            self.log("Субтитры: " + ", ".join(sorted(f.name[len(p.stem) + 1:-4] for f in got)))
        else:
            self.log("Субтитры: не найдены или YouTube временно ограничил запросы (видео сохранено).")

    # ---------- копия для монтажа ----------
    def make_edit_copy(self, src: str, info: dict | None = None, fmt: str | None = None,
                       fps: int | None = None, dst: Path | None = None, label: str = "") -> str:
        fmt = fmt or self.s.edit
        src_p = Path(src)
        probe = _probe(src)
        v = next((st for st in probe.get("streams", []) if st.get("codec_type") == "video"), {})
        duration = float(probe.get("format", {}).get("duration") or 0)
        hdr = v.get("color_transfer") in ("smpte2084", "arib-std-b67")
        has_audio = any(st.get("codec_type") == "audio" for st in probe.get("streams", []))

        vf = [f"fps={fps}"] if fps else []
        if hdr:
            # HDR (PQ/HLG, BT.2020) → SDR BT.709, иначе в AE/Premiere будут блёклые цвета.
            vf.append("zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,"
                      "tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv")

        if fmt == "prores":
            dst = dst or src_p.with_name(src_p.stem + "_edit.mov")
            vf.append("format=yuv422p10le")
            codec = ["-c:v", "prores_ks", "-profile:v", "3", "-vendor", "apl0"]
            acodec = ["-c:a", "pcm_s16le"]
        else:
            dst = dst or src_p.with_name(src_p.stem + "_edit.mp4")
            vf.append("format=yuv420p")
            codec = ["-c:v", "libx264", "-preset", "medium", "-crf", "16",
                     "-profile:v", "high", "-movflags", "+faststart"]
            acodec = ["-c:a", "aac", "-b:a", "320k"]

        cmd = ["ffmpeg", "-hide_banner", "-y", "-i", str(src_p), "-map", "0:v:0"]
        if has_audio:
            cmd += ["-map", "0:a:0"]
        cmd += ["-vf", ",".join(vf), *codec,
                # Постоянная частота кадров — AE и Premiere не любят VFR.
                "-fps_mode", "cfr",
                "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"]
        if has_audio:
            cmd += [*acodec, "-ar", "48000"]
        cmd += ["-progress", "pipe:1", "-nostats", str(dst)]

        label = (label or EDIT_FORMATS[fmt]) + (" (HDR→SDR)" if hdr else "")
        self._emit(stage="edit", percent=0.0, speed="", eta="", title=(info or {}).get("title", src_p.stem),
                   message=f"Перекодирую: {label}")
        self.log(f"Перекодирую: {dst.name} ({label})")
        # stderr сливаем в stdout: если его не читать, буфер канала переполняется и ffmpeg зависает.
        self._ffmpeg = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                        text=True, encoding="utf-8", errors="replace",
                                        creationflags=NO_WINDOW)
        speed = ""
        tail: deque[str] = deque(maxlen=30)
        for line in self._ffmpeg.stdout:
            key, sep, val = line.strip().partition("=")
            if not sep or " " in key:
                tail.append(line.rstrip())
            elif key == "speed":
                speed = val
            elif key == "out_time_us" and duration and val.isdigit():
                done = int(val) / 1e6
                sp = float(speed.rstrip("x") or 0) if speed not in ("", "N/A") else 0
                self._emit(percent=min(done / duration * 100, 100), speed=speed,
                           eta=_fmt_eta((duration - done) / sp) if sp else "")
        err = "\n".join(tail)
        rc = self._ffmpeg.wait()
        if self.cancelled:
            dst.unlink(missing_ok=True)
            raise Cancelled()
        if rc != 0:
            dst.unlink(missing_ok=True)
            raise RuntimeError(f"ffmpeg завершился с ошибкой:\n{err[-1500:]}")
        return str(dst)


class _Logger:
    def __init__(self, log: Callable[[str], None]):
        self.log = log
        self.errors = 0

    def debug(self, msg: str) -> None:
        if not msg.startswith("[debug]"):
            self.log(msg)

    def info(self, msg: str) -> None:
        self.log(msg)

    def warning(self, msg: str) -> None:
        self.log("Предупреждение: " + msg)

    def error(self, msg: str) -> None:
        self.errors += 1
        self.log("Ошибка: " + msg)


class _FinishPP(yt_dlp.postprocessor.PostProcessor):
    """after_move: видео уже на своём месте — доделываем его, не дожидаясь всего плейлиста."""

    def __init__(self, dl: "Downloader"):
        super().__init__()
        self.dl = dl

    def run(self, info):
        path = info.get("filepath")
        if path and Path(path).exists():
            try:
                self.dl.finish_video(info, path)
            except Cancelled:
                raise
            except Exception as e:  # noqa: BLE001 — видео уже скачано, не теряем его
                self.dl._logger.error(f"обработка {Path(path).name}: {str(e).splitlines()[0][:300]}")
        return [], info


def _check_disk(folder: str) -> None:
    root = Path(folder)
    while not root.exists() and root.parent != root:
        root = root.parent
    free = shutil.disk_usage(root).free / 2**30
    if free < MIN_FREE_GB:
        raise Cancelled(f"На диске осталось {free:.1f} ГБ (минимум {MIN_FREE_GB}) — загрузка остановлена.")


def _video_fps(path: str) -> float:
    v = next((st for st in _probe(path).get("streams", []) if st.get("codec_type") == "video"), {})
    num, _, den = (v.get("avg_frame_rate") or v.get("r_frame_rate") or "0/1").partition("/")
    try:
        return float(num) / float(den or 1)
    except (ValueError, ZeroDivisionError):
        return 0.0


def _iter_videos(info: dict):
    if info.get("_type") in ("playlist", "multi_video"):
        for e in info.get("entries") or []:
            if e:
                yield from _iter_videos(e)
    else:
        yield info


def _final_path(info: dict) -> str | None:
    for d in info.get("requested_downloads") or []:
        p = d.get("filepath")
        if p:
            return p
    return info.get("filepath") or info.get("_filename")


def _describe(info: dict) -> str:
    parts = []
    if info.get("width") and info.get("height"):
        parts.append(f"{info['width']}x{info['height']}")
    if info.get("fps"):
        parts.append(f"{info['fps']:g} fps")
    if info.get("vcodec") and info["vcodec"] != "none":
        parts.append(info["vcodec"].split(".")[0])
    if info.get("dynamic_range") and info["dynamic_range"] != "SDR":
        parts.append(info["dynamic_range"])
    if info.get("acodec") and info["acodec"] != "none":
        parts.append(info["acodec"].split(".")[0])
    return ", ".join(parts)


def _probe(path: str) -> dict:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", path],
                         capture_output=True, text=True, encoding="utf-8", creationflags=NO_WINDOW)
    return json.loads(out.stdout or "{}")


def update_ytdlp(log: Callable[[str], None] = print) -> str:
    """Обновляет yt-dlp (YouTube часто ломает старые версии)."""
    if FROZEN:
        return _update_frozen(log)
    r = subprocess.run([sys.executable, "-m", "pip", "install", "-U", "-q", "yt-dlp[default]"],
                       capture_output=True, text=True, creationflags=NO_WINDOW)
    if r.returncode != 0:
        raise RuntimeError(r.stderr[-1500:])
    v = subprocess.run([sys.executable, "-m", "yt_dlp", "--version"], capture_output=True, text=True,
                       creationflags=NO_WINDOW).stdout.strip()
    log(f"yt-dlp: {v}")
    return v


def _update_frozen(log: Callable[[str], None]) -> str:
    """В собранной программе нет pip: качаем колёса yt-dlp и yt-dlp-ejs с PyPI и распаковываем."""
    import io
    import urllib.request
    import zipfile

    def wheel(pkg: str, version: str | None = None) -> tuple[str, dict]:
        url = f"https://pypi.org/pypi/{pkg}/{version + '/' if version else ''}json"
        with urllib.request.urlopen(url, timeout=30) as r:
            meta = json.load(r)
        files = [f for f in meta["urls"] if f["filename"].endswith("py3-none-any.whl")]
        if not files:
            raise RuntimeError(f"На PyPI нет подходящего пакета {pkg}")
        with urllib.request.urlopen(files[0]["url"], timeout=120) as r:
            return meta["info"]["version"], {"data": r.read(), "requires": meta["info"].get("requires_dist") or []}

    log("Проверяю новую версию yt-dlp…")
    version, main = wheel("yt-dlp")
    current = yt_dlp.version.__version__
    if vtuple(version) <= vtuple(current):
        log(f"yt-dlp уже последней версии: {current}")
        return current
    tmp = PYLIB_DIR.with_name("pylib_new")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    zipfile.ZipFile(io.BytesIO(main["data"])).extractall(tmp)
    # yt-dlp-ejs (решение JS-задач YouTube) — ровно той версии, которую требует новый yt-dlp.
    ejs = next((r for r in main["requires"] if r.replace(" ", "").startswith("yt-dlp-ejs==")), None)
    if ejs:
        _, extra = wheel("yt-dlp-ejs", ejs.split("==", 1)[1].split(";", 1)[0].strip())
        zipfile.ZipFile(io.BytesIO(extra["data"])).extractall(tmp)
    shutil.rmtree(PYLIB_DIR, ignore_errors=True)
    tmp.rename(PYLIB_DIR)
    log(f"yt-dlp: {current} → {version}")
    return version
