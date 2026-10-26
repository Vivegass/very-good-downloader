"""Сортировка видео по папкам: Тема / Канал.

Тема определяется по ключевым словам (название, теги, канал) из topics.json,
если ничего не совпало — по категории YouTube. Каналы с похожими никами
(«STEALTH X» и «Stealth X Edits») попадают в одну папку.
"""
from __future__ import annotations

import json
import os
import re
import shutil
from difflib import SequenceMatcher
from pathlib import Path
from typing import Callable

from appinfo import DATA_DIR  # noqa: E402

TOPICS_FILE = DATA_DIR / "topics.json"
INDEX_NAME = ".ytdl_index.json"
ID_RE = re.compile(r"\[([A-Za-z0-9_-]{11})\]")

# Порядок важен: при равном счёте побеждает тема выше.
DEFAULT_TOPICS: dict[str, list[str]] = {
    "Эдиты": ["edit", "edits", "эдит", "эдиты", "amv", "mmv", "montage", "монтаж", "velocity", "twixtor",
              "quickedit", "flow edit", "trickshot", "trickshotting", "clips in desc", "edit contest"],
    "Туториалы": ["tutorial", "туториал", "урок", "how to", "как сделать", "guide", "гайд", "курс",
                  "breakdown", "разбор"],
    "Моушн и VFX": ["motion design", "моушн", "vfx", "cgi", "after effects", "blender", "houdini",
                    "cinema 4d", "c4d", "3d animation", "render", "embergen"],
    "Игры": ["gameplay", "геймплей", "прохождение", "let's play", "speedrun", "beamng", "cs2",
             "counter-strike", "minecraft", "gta", "valorant", "fortnite", "assetto corsa", "iracing",
             "dota", "rust"],
    "Музыка": ["official music video", "music video", "official audio", "клип", "remix", "lyrics",
               "phonk", "album", "альбом", "концерт", "live performance"],
    "Кино": ["trailer", "трейлер", "movie", "фильм", "film", "scene", "сцена", "сериал"],
}

CATEGORY_TOPICS = {
    "Gaming": "Игры", "Music": "Музыка", "Film & Animation": "Кино", "Education": "Туториалы",
    "Howto & Style": "Туториалы", "Science & Technology": "Технологии", "Autos & Vehicles": "Авто",
    "Sports": "Спорт", "Comedy": "Юмор", "Entertainment": "Развлечения", "People & Blogs": "Блоги",
    "News & Politics": "Новости", "Travel & Events": "Путешествия", "Pets & Animals": "Животные",
}
FALLBACK_TOPIC = "Разное"

# Слова, которые не отличают один канал от другого: «X Edits» = «X» = «X Official».
_NICK_NOISE = {"official", "officiel", "channel", "tv", "yt", "youtube", "edit", "edits", "clips", "clip",
               "shorts", "music", "gaming", "games", "hd", "4k", "studio", "studios", "archive", "backup",
               "vevo", "topic", "канал", "официальный", "эдиты", "эдит", "клипы", "the"}


def load_topics() -> dict[str, list[str]]:
    try:
        data = json.loads(TOPICS_FILE.read_text("utf-8"))
        if isinstance(data, dict) and data:
            return {str(k): [str(w).lower() for w in v] for k, v in data.items()}
    except FileNotFoundError:
        ensure_topics_file()
    except Exception:  # noqa: BLE001 — битый файл: работаем со стандартными темами
        pass
    return DEFAULT_TOPICS


def ensure_topics_file() -> Path:
    if not TOPICS_FILE.exists():
        TOPICS_FILE.parent.mkdir(parents=True, exist_ok=True)
        TOPICS_FILE.write_text(json.dumps(DEFAULT_TOPICS, ensure_ascii=False, indent=2), "utf-8")
    return TOPICS_FILE


def _has(text: str, kw: str) -> int:
    return len(re.findall(r"(?<!\w)" + re.escape(kw) + r"(?!\w)", text))


def detect_topic(info: dict, topics: dict[str, list[str]] | None = None) -> str:
    topics = topics or load_topics()
    title = (info.get("title") or "").lower()
    tags = " | ".join(info.get("tags") or []).lower()
    chan = (info.get("channel") or info.get("uploader") or "").lower()
    # Плейлист «Very Good Edits» — сильная подсказка для всех его видео.
    pl = (info.get("playlist_title") or info.get("playlist") or "").lower()
    best, best_score = None, 0
    for topic, words in topics.items():
        score = sum(_has(title, w) * 3 + min(_has(tags, w), 3) + _has(chan, w) * 2 + _has(pl, w) * 4 for w in words)
        if score > best_score:
            best, best_score = topic, score
    if best:
        return best
    for cat in info.get("categories") or []:
        if cat in CATEGORY_TOPICS:
            return CATEGORY_TOPICS[cat]
    return FALLBACK_TOPIC


def safe_name(name: str, limit: int = 80) -> str:
    name = re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", name).strip().rstrip(". ")
    return name[:limit].rstrip(". ") or "Без названия"


def _nick_tokens(name: str) -> list[str]:
    tokens = re.findall(r"\w+", name.lower().replace("ё", "е"))
    core = [t for t in tokens if t not in _NICK_NOISE]
    return core or tokens


def similar_nicks(a: str, b: str) -> bool:
    ta, tb = _nick_tokens(a), _nick_tokens(b)
    ka, kb = "".join(ta), "".join(tb)
    if not ka or not kb:
        return False
    if ka == kb:
        return True
    # «troubled duck» и «troubled duck and plugdi»: короткий ник — начало длинного целыми словами.
    short, long_ = sorted((ta, tb), key=len)
    if len("".join(short)) >= 4 and long_[:len(short)] == short:
        return True
    # Опечатки и вариации написания длинных ников.
    return min(len(ka), len(kb)) >= 6 and SequenceMatcher(None, ka, kb).ratio() >= 0.9


class Sorter:
    """Хранит соответствие канал → папка в .ytdl_index.json внутри папки загрузок."""

    def __init__(self, root: str | Path):
        self.root = Path(root)
        self.index_path = self.root / INDEX_NAME
        try:
            self.index = json.loads(self.index_path.read_text("utf-8"))
        except Exception:  # noqa: BLE001
            self.index = {}
        self.index.setdefault("channels", {})
        self.topics = load_topics()

    def save(self) -> None:
        self.root.mkdir(parents=True, exist_ok=True)
        self.index_path.write_text(json.dumps(self.index, ensure_ascii=False, indent=1), "utf-8")

    def channel_folder(self, info: dict) -> str:
        cid = info.get("channel_id") or info.get("uploader_id") or ""
        name = info.get("channel") or info.get("uploader") or "Неизвестный канал"
        chans: dict = self.index["channels"]
        if cid and cid in chans:
            return chans[cid]
        folder = next((f for f in dict.fromkeys(chans.values()) if similar_nicks(name, f)), None)
        if folder is None:
            # Папки, созданные раньше вручную или без индекса, тоже учитываем.
            existing = {p.name for t in self.root.iterdir() if t.is_dir() for p in t.iterdir() if p.is_dir()} \
                if self.root.exists() else set()
            folder = next((f for f in existing if similar_nicks(name, f)), safe_name(name))
        if cid:
            chans[cid] = folder
            self.save()
        return folder

    def target_dir(self, info: dict) -> tuple[str, str]:
        return safe_name(detect_topic(info, self.topics)), self.channel_folder(info)


# ---------- разложить уже скачанное ----------
def plan_existing(root: str | Path, log: Callable[[str], None] = print,
                  cancelled: Callable[[], bool] = lambda: False) -> list[tuple[Path, Path]]:
    """Ищет в корне папки файлы с [id] в имени и решает, куда их переложить."""
    import yt_dlp
    root = Path(root)
    groups: dict[str, list[Path]] = {}
    for f in root.iterdir():
        m = ID_RE.search(f.name) if f.is_file() else None
        if m:
            groups.setdefault(m.group(1), []).append(f)
    sorter = Sorter(root)
    plan: list[tuple[Path, Path]] = []
    opts = {"quiet": True, "no_warnings": True, "skip_download": True, "noplaylist": True}
    with yt_dlp.YoutubeDL(opts) as ydl:
        for i, (vid, files) in enumerate(groups.items(), 1):
            if cancelled():
                break
            log(f"[{i}/{len(groups)}] Узнаю канал и тему: {vid}")
            try:
                info = ydl.extract_info(f"https://www.youtube.com/watch?v={vid}", download=False, process=False)
            except Exception as e:  # noqa: BLE001
                log(f"  пропускаю {vid}: {str(e).splitlines()[0][:120]}")
                continue
            topic, chan = sorter.target_dir(info)
            for f in files:
                plan.append((f, root / topic / chan / f.name))
    return plan


def apply_plan(plan: list[tuple[Path, Path]], log: Callable[[str], None] = print) -> int:
    moved = 0
    for src, dst in plan:
        if not src.exists() or dst.exists():
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), str(dst))
        moved += 1
    log(f"Перемещено файлов: {moved}")
    return moved
