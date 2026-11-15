"""Mock GitHub Releases для проверки обновления Android на эмуляторе (только отладка).

    python android/tools/update-mock.py <папка с VeryGoodDownloader-android-<ver>-arm64.apk> <ver> [порт]
    adb reverse tcp:8767 tcp:8767
    adb shell "run-as io.github.vivegass.vgd.debug sh -c 'echo http://127.0.0.1:8767/releases > files/update_url.txt'"

Адреса файлов в ответе — MOCK_BASE (по умолчанию http://127.0.0.1:<порт>, через adb reverse). Через NAT
эмулятора (10.0.2.2) большие файлы с Python-сервера на Windows приходили испорченными — SHA-256 не сходился.
Debug-сборка читает files/update_url.txt и ставит VGD_UPDATE_URL; release его не читает.
"""
import hashlib, json, os, socket, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DIR = sys.argv[1]
VER = sys.argv[2]
PORT = int(sys.argv[3]) if len(sys.argv) > 3 else 8767
BASE = os.environ.get("MOCK_BASE", f"http://127.0.0.1:{PORT}")

assets = []
for n in os.listdir(DIR):
    p = os.path.join(DIR, n)
    h = hashlib.sha256(open(p, "rb").read()).hexdigest()
    assets.append({"name": n, "size": os.path.getsize(p), "browser_download_url": f"{BASE}/{n}", "digest": f"sha256:{h}"})
rel = [{
    "tag_name": f"android-v{VER}", "name": f"Very Good Downloader для Android {VER}", "draft": False, "prerelease": False,
    "html_url": f"{BASE}/page", "published_at": "2026-11-15T00:00:00Z",
    "body": f"## Что нового в {VER} (mock)\n\n- проверка обновления поверх\n- плашка, уведомление, PackageInstaller",
    "assets": assets,
}]
body = json.dumps(rel, ensure_ascii=False).encode()


class H(BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?")[0].lstrip("/")
        print("GET", path, self.headers.get("Range"), flush=True)
        if path == "releases":
            self.send_response(200)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        f = os.path.join(DIR, path)
        if not os.path.isfile(f):
            self.send_response(404)
            self.end_headers()
            return
        data = open(f, "rb").read()
        start = 0
        r = self.headers.get("Range")
        if r and r.startswith("bytes="):
            start = int(r[6:].split("-")[0] or 0)
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {start}-{len(data) - 1}/{len(data)}")
        else:
            self.send_response(200)
        self.send_header("Content-Type", "application/vnd.android.package-archive")
        self.send_header("Content-Length", str(len(data) - start))
        self.end_headers()
        self.wfile.write(data[start:])
        self.wfile.flush()
        # Windows: закрыть сокет без shutdown — RST, и хвост ответа теряется по пути (NAT эмулятора)
        self.connection.shutdown(socket.SHUT_WR)

    def log_message(self, *a):
        pass


print(f"mock: {BASE}/releases", flush=True)
ThreadingHTTPServer(("0.0.0.0", PORT), H).serve_forever()
