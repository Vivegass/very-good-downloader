"""Окно программы: очередь ссылок, прогресс, настройки."""
from __future__ import annotations

import os
import queue
import subprocess
import threading
import tkinter as tk
from dataclasses import dataclass, field
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from ytdl_core import (BROWSERS, EDIT_FORMATS, FPS_LIMITS, HEIGHTS, Cancelled, Downloader, Progress, Settings,
                       update_ytdlp)
from ytdl_sort import apply_plan, ensure_topics_file, plan_existing
from appinfo import APP_DIR, APP_NAME, APP_VERSION

BROWSER_NAMES = {"none": "Не использовать", "firefox": "Firefox", "chrome": "Chrome", "edge": "Edge",
                 "brave": "Brave", "opera": "Opera", "vivaldi": "Vivaldi", "chromium": "Chromium",
                 "file": "Файл cookies.txt…"}
HEIGHT_NAMES = {0: "Максимум", 2160: "4K (2160p)", 1440: "1440p", 1080: "Full HD (1080p)", 720: "720p", 480: "480p"}
FPS_NAMES = {0: "Без ограничения", 60: "60", 30: "30", 25: "25", 24: "24"}
STAGES = {"video": "Видео", "audio": "Звук", "merge": "Склейка", "edit": "Перекодирование", "subs": "Субтитры",
          "done": "Готово", "error": "Ошибка", "wait": "В очереди", "cancel": "Отменено", "info": "Подготовка"}


@dataclass
class Job:
    url: str
    iid: str = ""
    files: list[str] = field(default_factory=list)
    state: str = "wait"


class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title(f"{APP_NAME} {APP_VERSION}")
        try:
            self.iconbitmap(default=str(APP_DIR / "app.ico"))
        except tk.TclError:
            pass
        self.geometry("1000x760")
        self.minsize(820, 560)
        self.s = Settings.load()
        self.jobs: list[Job] = []
        self.events: queue.Queue = queue.Queue()
        self.worker: threading.Thread | None = None
        self.current: Downloader | None = None
        self.stop_all = False

        style = ttk.Style(self)
        if "vista" in style.theme_names():
            style.theme_use("vista")
        style.configure("Big.TButton", padding=(14, 6))
        self._build()
        self.after(100, self._poll)
        self.protocol("WM_DELETE_WINDOW", self._close)

    # ---------- интерфейс ----------
    def _build(self) -> None:
        pad = {"padx": 10, "pady": 4}

        top = ttk.LabelFrame(self, text="Ссылки (по одной на строку: видео, плейлисты, каналы)")
        top.pack(fill="x", **pad)
        self.urls = tk.Text(top, height=4, wrap="none", font=("Segoe UI", 10), undo=True)
        self.urls.pack(side="left", fill="both", expand=True, padx=(8, 4), pady=8)
        btns = ttk.Frame(top)
        btns.pack(side="right", fill="y", padx=(4, 8), pady=8)
        ttk.Button(btns, text="Вставить", command=self._paste).pack(fill="x")
        ttk.Button(btns, text="Скачать", style="Big.TButton", command=self._add_and_start).pack(fill="x", pady=(6, 0))

        opt = ttk.LabelFrame(self, text="Настройки")
        opt.pack(fill="x", **pad)
        opt.columnconfigure(1, weight=1)

        ttk.Label(opt, text="Папка:").grid(row=0, column=0, sticky="w", padx=8, pady=4)
        self.v_out = tk.StringVar(value=self.s.out_dir)
        ttk.Entry(opt, textvariable=self.v_out).grid(row=0, column=1, columnspan=3, sticky="ew", pady=4)
        ttk.Button(opt, text="Выбрать…", command=self._choose_dir).grid(row=0, column=4, padx=4)
        ttk.Button(opt, text="Открыть", command=lambda: self._open(self.v_out.get())).grid(row=0, column=5, padx=(0, 8))

        ttk.Label(opt, text="Копия для монтажа:").grid(row=1, column=0, sticky="w", padx=8, pady=4)
        self.v_edit = tk.StringVar(value=EDIT_FORMATS[self.s.edit])
        ttk.Combobox(opt, textvariable=self.v_edit, values=list(EDIT_FORMATS.values()), state="readonly",
                     width=28).grid(row=1, column=1, sticky="w")

        ttk.Label(opt, text="Куки (18+, закрытые):").grid(row=1, column=2, sticky="e", padx=(16, 4))
        self.v_cookies = tk.StringVar(value=BROWSER_NAMES[self.s.cookies])
        cb = ttk.Combobox(opt, textvariable=self.v_cookies, values=[BROWSER_NAMES[b] for b in BROWSERS],
                          state="readonly", width=20)
        cb.grid(row=1, column=3, sticky="w")
        cb.bind("<<ComboboxSelected>>", self._on_cookies)
        self.v_cookies_file = tk.StringVar(value=self.s.cookies_file)

        ttk.Label(opt, text="Макс. разрешение:").grid(row=2, column=0, sticky="w", padx=8, pady=4)
        rowq = ttk.Frame(opt)
        rowq.grid(row=2, column=1, columnspan=5, sticky="w")
        self.v_height = tk.StringVar(value=HEIGHT_NAMES.get(self.s.max_height, "Максимум"))
        ttk.Combobox(rowq, textvariable=self.v_height, values=[HEIGHT_NAMES[h] for h in HEIGHTS],
                     state="readonly", width=18).pack(side="left")
        ttk.Label(rowq, text="Макс. FPS:").pack(side="left", padx=(16, 4))
        self.v_fps = tk.StringVar(value=FPS_NAMES.get(self.s.max_fps, "Без ограничения"))
        ttk.Combobox(rowq, textvariable=self.v_fps, values=[FPS_NAMES[f] for f in FPS_LIMITS],
                     state="readonly", width=16).pack(side="left")
        ttk.Label(rowq, text="(если нужного fps нет — пересчёт в H.264)", foreground="#777").pack(side="left", padx=8)
        row2 = ttk.Frame(opt)
        row2.grid(row=3, column=0, columnspan=6, sticky="w", padx=8, pady=(4, 2))
        self.v_pl = tk.BooleanVar(value=self.s.playlist)
        self.v_subs = tk.BooleanVar(value=self.s.subs)
        self.v_auto = tk.BooleanVar(value=self.s.auto_subs)
        self.v_thumb = tk.BooleanVar(value=self.s.thumbnail)
        self.v_langs = tk.StringVar(value=self.s.sub_langs)
        ttk.Checkbutton(row2, text="Плейлисты и каналы целиком", variable=self.v_pl).pack(side="left")
        ttk.Checkbutton(row2, text="Обложка", variable=self.v_thumb).pack(side="left", padx=(16, 0))
        ttk.Checkbutton(row2, text="Субтитры:", variable=self.v_subs).pack(side="left", padx=(16, 4))
        ttk.Entry(row2, textvariable=self.v_langs, width=10).pack(side="left")
        ttk.Checkbutton(row2, text="включая автосубтитры", variable=self.v_auto).pack(side="left", padx=(8, 0))
        row3 = ttk.Frame(opt)
        row3.grid(row=4, column=0, columnspan=6, sticky="w", padx=8, pady=(2, 8))
        self.v_sort = tk.BooleanVar(value=self.s.sort)
        ttk.Checkbutton(row3, text="Раскладывать по папкам: Тема / Канал (похожие ники — в одну папку)",
                        variable=self.v_sort).pack(side="left")
        ttk.Button(row3, text="Темы…", command=self._edit_topics).pack(side="left", padx=(12, 0))
        ttk.Button(row3, text="Разложить папку…", command=self._sort_existing).pack(side="left", padx=(6, 0))

        mid = ttk.LabelFrame(self, text="Очередь")
        mid.pack(fill="both", expand=True, **pad)
        cols = ("title", "stage", "pct", "speed", "eta")
        self.tree = ttk.Treeview(mid, columns=cols, show="headings", height=8, selectmode="extended")
        for c, t, w, anchor in (("title", "Название / ссылка", 420, "w"), ("stage", "Этап", 210, "w"),
                                ("pct", "%", 60, "e"), ("speed", "Скорость", 100, "e"), ("eta", "Осталось", 80, "e")):
            self.tree.heading(c, text=t)
            self.tree.column(c, width=w, anchor=anchor, stretch=(c == "title"))
        sb = ttk.Scrollbar(mid, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side="left", fill="both", expand=True, padx=(8, 0), pady=8)
        sb.pack(side="left", fill="y", pady=8, padx=(0, 8))
        self.tree.bind("<Double-1>", self._reveal_selected)
        self.tree.tag_configure("done", foreground="#1a7f37")
        self.tree.tag_configure("error", foreground="#c62828")
        self.tree.tag_configure("cancel", foreground="#888888")

        bar = ttk.Frame(self)
        bar.pack(fill="x", **pad)
        self.pb = ttk.Progressbar(bar, mode="determinate", maximum=100)
        self.pb.pack(side="left", fill="x", expand=True)
        ttk.Button(bar, text="Отменить", command=self._cancel).pack(side="left", padx=(8, 0))
        ttk.Button(bar, text="Убрать готовые", command=self._clear_done).pack(side="left", padx=(6, 0))
        ttk.Button(bar, text="Обновить yt-dlp", command=self._update).pack(side="left", padx=(6, 0))

        logf = ttk.LabelFrame(self, text="Журнал")
        logf.pack(fill="both", **pad)
        self.log_box = tk.Text(logf, height=7, wrap="word", font=("Consolas", 9), state="disabled",
                               background="#f6f6f6")
        self.log_box.pack(fill="both", expand=True, padx=8, pady=8)

        self.status = ttk.Label(self, text="Вставьте ссылку и нажмите «Скачать». "
                                           "Двойной щелчок по готовому видео покажет его в папке.",
                                foreground="#666")
        self.status.pack(fill="x", padx=12, pady=(0, 8))

        self.bind("<Control-Return>", lambda e: self._add_and_start())

    # ---------- действия ----------
    def _paste(self) -> None:
        try:
            text = self.clipboard_get().strip()
        except tk.TclError:
            return
        if text:
            cur = self.urls.get("1.0", "end").strip()
            self.urls.insert("end", ("\n" if cur else "") + text)

    def _choose_dir(self) -> None:
        d = filedialog.askdirectory(initialdir=self.v_out.get() or str(Path.home()))
        if d:
            self.v_out.set(os.path.normpath(d))

    def _on_cookies(self, _e=None) -> None:
        if self._cookies_key() == "file":
            f = filedialog.askopenfilename(title="Файл cookies.txt (формат Netscape)",
                                           filetypes=[("cookies.txt", "*.txt"), ("Все файлы", "*.*")])
            if f:
                self.v_cookies_file.set(f)
            else:
                self.v_cookies.set(BROWSER_NAMES["none"])

    def _cookies_key(self) -> str:
        return next(k for k, v in BROWSER_NAMES.items() if v == self.v_cookies.get())

    def _collect_settings(self) -> Settings:
        s = self.s
        s.out_dir = self.v_out.get().strip() or s.out_dir
        s.edit = next(k for k, v in EDIT_FORMATS.items() if v == self.v_edit.get())
        s.cookies = self._cookies_key()
        s.cookies_file = self.v_cookies_file.get()
        s.playlist, s.subs, s.auto_subs = self.v_pl.get(), self.v_subs.get(), self.v_auto.get()
        s.thumbnail, s.sub_langs = self.v_thumb.get(), self.v_langs.get()
        s.sort = self.v_sort.get()
        s.max_height = next(k for k, v in HEIGHT_NAMES.items() if v == self.v_height.get())
        s.max_fps = next(k for k, v in FPS_NAMES.items() if v == self.v_fps.get())
        s.save()
        return Settings(**vars(s))

    def _add_and_start(self) -> None:
        lines = [l.strip() for l in self.urls.get("1.0", "end").splitlines() if l.strip()]
        if not lines:
            self._paste()
            lines = [l.strip() for l in self.urls.get("1.0", "end").splitlines() if l.strip()]
        if not lines:
            return
        for url in lines:
            job = Job(url)
            job.iid = self.tree.insert("", "end", values=(url, STAGES["wait"], "", "", ""))
            self.jobs.append(job)
        self.urls.delete("1.0", "end")
        self._collect_settings()
        self._start_worker()

    def _start_worker(self) -> None:
        if self.worker and self.worker.is_alive():
            return
        self.stop_all = False
        self.worker = threading.Thread(target=self._work, daemon=True)
        self.worker.start()

    def _work(self) -> None:
        while not self.stop_all:
            job = next((j for j in self.jobs if j.state == "wait"), None)
            if job is None:
                break
            job.state = "info"
            settings = self._collect_settings_threadsafe()
            dl = Downloader(settings,
                            on_progress=lambda p, j=job: self.events.put(("progress", j, Progress(**vars(p)))),
                            log=lambda m: self.events.put(("log", None, m)))
            self.current = dl
            try:
                job.files = dl.run(job.url)
                job.state = "done"
                self.events.put(("finish", job, None))
            except Cancelled:
                job.state = "cancel"
                self.events.put(("finish", job, None))
            except Exception as e:  # noqa: BLE001
                job.state = "error"
                self.events.put(("log", None, f"Ошибка ({job.url}): {e}"))
                self.events.put(("finish", job, str(e)))
            finally:
                self.current = None
        self.events.put(("idle", None, None))

    def _collect_settings_threadsafe(self) -> Settings:
        # Настройки читаются из сохранённого объекта, а не из виджетов (их трогает только главный поток).
        return Settings(**vars(self.s))

    def _cancel(self) -> None:
        sel = [j for j in self.jobs if j.iid in self.tree.selection() and j.state == "wait"]
        for j in sel:  # выделенные ожидающие — просто убрать из очереди
            j.state = "cancel"
            self._set_row(j, stage=STAGES["cancel"], tag="cancel")
        if not sel and self.current:
            self.current.cancel()
            self.status.config(text="Отменяю текущую загрузку…")

    def _clear_done(self) -> None:
        for j in [j for j in self.jobs if j.state in ("done", "cancel", "error")]:
            self.tree.delete(j.iid)
            self.jobs.remove(j)

    def _update(self) -> None:
        if self.worker and self.worker.is_alive():
            messagebox.showinfo("Обновление", "Дождитесь окончания загрузок.")
            return
        self.status.config(text="Обновляю yt-dlp…")

        def run():
            try:
                v = update_ytdlp(lambda m: self.events.put(("log", None, m)))
                self.events.put(("status", None, f"yt-dlp обновлён до {v}. Перезапустите программу."))
            except Exception as e:  # noqa: BLE001
                self.events.put(("status", None, f"Не удалось обновить: {e}"))
        threading.Thread(target=run, daemon=True).start()

    def _edit_topics(self) -> None:
        f = ensure_topics_file()
        messagebox.showinfo("Темы", "Откроется файл с темами: название темы и ключевые слова.\n"
                                    "Тема выбирается по совпадениям в названии, тегах, нике канала и плейлисте;\n"
                                    "если совпадений нет — по категории YouTube.\nПосле правки сохраните файл.")
        os.startfile(f)

    def _sort_existing(self) -> None:
        if self.worker and self.worker.is_alive():
            messagebox.showinfo("Разложить", "Дождитесь окончания загрузок.")
            return
        root = self.v_out.get().strip()
        if not root or not Path(root).exists():
            return
        self.status.config(text="Ищу видео с [id] в имени и узнаю их каналы…")

        def work():
            try:
                plan = plan_existing(root, log=lambda m: self.events.put(("log", None, m)))
                self.events.put(("sortplan", None, plan))
            except Exception as e:  # noqa: BLE001
                self.events.put(("status", None, f"Не удалось: {e}"))
        threading.Thread(target=work, daemon=True).start()

    def _show_sort_plan(self, plan) -> None:
        root = Path(self.v_out.get().strip())
        if not plan:
            messagebox.showinfo("Разложить", "В корне папки нет неразложенных видео с [id] в имени.")
            self.status.config(text="Раскладывать нечего.")
            return
        win = tk.Toplevel(self)
        win.title(f"Разложить: {len(plan)} файл(ов)")
        win.geometry("860x480")
        txt = tk.Text(win, wrap="none", font=("Consolas", 9))
        txt.pack(fill="both", expand=True, padx=8, pady=8)
        for src, dst in plan:
            txt.insert("end", f"{dst.parent.relative_to(root)}  ←  {src.name}\n")
        txt.configure(state="disabled")
        bar = ttk.Frame(win)
        bar.pack(fill="x", padx=8, pady=(0, 8))

        def go():
            n = apply_plan(plan, log=self._log)
            self.status.config(text=f"Разложено файлов: {n}")
            win.destroy()
        ttk.Button(bar, text="Переместить", style="Big.TButton", command=go).pack(side="right")
        ttk.Button(bar, text="Отмена", command=win.destroy).pack(side="right", padx=6)

    def _reveal_selected(self, _e=None) -> None:
        for j in self.jobs:
            if j.iid in self.tree.selection() and j.files:
                subprocess.Popen(["explorer", "/select,", os.path.normpath(j.files[0])])
                return

    def _open(self, path: str) -> None:
        Path(path).mkdir(parents=True, exist_ok=True)
        os.startfile(path)

    # ---------- события из рабочего потока ----------
    def _set_row(self, job: Job, title=None, stage=None, pct=None, speed=None, eta=None, tag=None) -> None:
        if not self.tree.exists(job.iid):
            return
        vals = list(self.tree.item(job.iid, "values"))
        for i, v in enumerate((title, stage, pct, speed, eta)):
            if v is not None:
                vals[i] = v
        self.tree.item(job.iid, values=vals, tags=(tag,) if tag else ())

    def _poll(self) -> None:
        try:
            while True:
                kind, job, data = self.events.get_nowait()
                if kind == "progress":
                    p: Progress = data
                    stage = STAGES.get(p.stage, "Подготовка")
                    if p.item:
                        stage = f"[{p.item}] {stage}"
                    self._set_row(job, title=p.title or job.url, stage=stage,
                                  pct=f"{p.percent:.0f}" if p.stage else "", speed=p.speed, eta=p.eta)
                    self.pb["value"] = p.percent
                    self.status.config(text=p.message)
                elif kind == "finish":
                    tag = job.state
                    self._set_row(job, stage=STAGES[job.state] + (f": {len(job.files)} файл(ов)" if job.files else ""),
                                  pct="100" if tag == "done" else "", speed="", eta="", tag=tag)
                    if data:
                        self.status.config(text=f"Ошибка: {data.splitlines()[0][:150]}")
                elif kind == "log":
                    self._log(data)
                elif kind == "sortplan":
                    self._show_sort_plan(data)
                elif kind == "status":
                    self.status.config(text=data)
                elif kind == "idle":
                    self.pb["value"] = 0
                    if not any(j.state == "error" for j in self.jobs):
                        self.status.config(text="Все загрузки завершены.")
        except queue.Empty:
            pass
        self.after(100, self._poll)

    def _log(self, msg: str) -> None:
        if msg.startswith("[download]"):
            return
        self.log_box.configure(state="normal")
        self.log_box.insert("end", msg + "\n")
        lines = int(self.log_box.index("end-1c").split(".")[0])
        if lines > 2000:
            self.log_box.delete("1.0", f"{lines - 2000}.0")
        self.log_box.see("end")
        self.log_box.configure(state="disabled")

    def _close(self) -> None:
        if self.worker and self.worker.is_alive():
            if not messagebox.askyesno("Выход", "Идёт загрузка. Прервать и выйти?"):
                return
            self.stop_all = True
            if self.current:
                self.current.cancel()
        try:
            self._collect_settings()
        except Exception:  # noqa: BLE001
            pass
        self.destroy()


if __name__ == "__main__":
    try:
        import ctypes
        ctypes.windll.shcore.SetProcessDpiAwareness(1)  # чёткий текст на мониторах с масштабом
    except Exception:  # noqa: BLE001
        pass
    App().mainloop()
