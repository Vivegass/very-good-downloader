# PyInstaller: окно (Very Good Downloader.exe) и консоль (vgd.exe) в одной папке.
# Сборка: build.ps1 (он же кладёт рядом bin\ffmpeg, ffprobe, deno).
from PyInstaller.utils.hooks import collect_all

SRC = r"C:\Users\gi235\Documents\YouTubeDownloader"
ICON = SRC + r"\app.ico"

datas, binaries, hidden = [(ICON, ".")], [], ["yt_dlp.compat._legacy", "yt_dlp.compat._deprecated"]
for pkg in ("yt_dlp_ejs", "curl_cffi", "certifi"):
    d, b, h = collect_all(pkg)
    datas += d
    binaries += b
    hidden += h

common = dict(pathex=[SRC], binaries=binaries, datas=datas, hiddenimports=hidden,
              excludes=["matplotlib", "numpy", "PIL", "pytest"])
a_gui = Analysis([SRC + r"\ytdl_gui.py"], **common)
a_cli = Analysis([SRC + r"\ytdl.py"], **common)

exe_gui = EXE(PYZ(a_gui.pure), a_gui.scripts, [], exclude_binaries=True, name="Very Good Downloader",
              console=False, icon=ICON, upx=False)
exe_cli = EXE(PYZ(a_cli.pure), a_cli.scripts, [], exclude_binaries=True, name="vgd",
              console=True, icon=ICON, upx=False)

coll = COLLECT(exe_gui, exe_cli,
               a_gui.binaries, a_gui.datas, a_cli.binaries, a_cli.datas,
               upx=False, name="VeryGoodDownloader")
