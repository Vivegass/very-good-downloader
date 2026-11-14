// Страница плейлиста open.spotify.com в «невидимом браузере» программы (на Android — скрытый WebView):
// прокручивает виртуальный список (видно ~25 строк) и отдаёт все треки. Та же логика, что у кнопки
// расширения «скачать плейлист» (extension/spotify.js). Вход не нужен: публичный плейлист виден всем.
// Ответ — window.ipc.postMessage(JSON): {type: "spotify", name, total, tracks: [...]} или {type: "spotify", error}.
(() => {
  if (window.__vgdSpotify) return;
  window.__vgdSpotify = 1;
  const post = (o) => window.ipc.postMessage(JSON.stringify(Object.assign({ type: "spotify" }, o)));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const ROW = '[data-testid="tracklist-row"]';

  const toSeconds = (s) => {
    if (!s) return null;
    const p = s.split(":").map(Number);
    return p.some(isNaN) ? null : p.reduce((a, b) => a * 60 + b, 0);
  };

  /** Сколько треков по заголовку («201 трек», «45 songs»). */
  function totalFromHeader() {
    for (const el of document.querySelectorAll("span, div")) {
      if (el.children.length) continue;
      const m = /^(\d[\d\s,.]*)\s*(трек|песн|song|track|item)/i.exec(el.textContent.trim());
      if (m) return parseInt(m[1].replace(/[\s,.]/g, ""), 10);
    }
    return 0;
  }

  function rowTrack(r) {
    const t = r.querySelector('a[href*="/track/"]');
    if (!t) return null;
    const id = t.getAttribute("href").split("/track/")[1].split(/[?#]/)[0];
    const row = r.closest("[aria-rowindex]");
    const artists = [...new Set([...r.querySelectorAll('a[href*="/artist/"]')].map((a) => a.textContent.trim()))];
    const album = r.querySelector('a[href*="/album/"]');
    const dur = [...r.querySelectorAll("div, span")].map((e) => e.textContent.trim()).find((x) => /^\d{1,2}:\d{2}(:\d{2})?$/.test(x));
    const img = r.querySelector("img");
    return {
      idx: row ? +row.getAttribute("aria-rowindex") : 0,
      id,
      title: t.textContent.trim(),
      artist: artists.join(", "),
      album: album ? album.textContent.trim() : "",
      duration: toSeconds(dur),
      cover: img && img.src && img.src.startsWith("http") ? img.src : undefined,
    };
  }

  function scroller() {
    let el = document.querySelector(ROW);
    while (el && el !== document.body) {
      const s = getComputedStyle(el);
      if ((s.overflowY === "auto" || s.overflowY === "scroll") && el.scrollHeight > el.clientHeight + 20) return el;
      el = el.parentElement;
    }
    return document.scrollingElement;
  }

  function listName() {
    const h = document.querySelector('[data-testid="entityTitle"], main h1');
    return (h && h.textContent.trim()) || document.title.replace(/\s*[-|].*$/, "").trim() || "Spotify";
  }

  async function run() {
    // список появляется не сразу (страница грузится скриптами)
    for (let i = 0; i < 80 && !document.querySelector(ROW); i++) await sleep(500);
    if (!document.querySelector(ROW)) return post({ error: "на странице Spotify нет списка треков (плейлист закрыт или страница не загрузилась)" });
    const seen = new Map();
    const total = totalFromHeader();
    const sc = scroller();
    const grab = () =>
      document.querySelectorAll(ROW).forEach((r) => {
        const t = rowTrack(r);
        if (t && !seen.has(t.id)) seen.set(t.id, t);
      });
    let last = -1;
    let stable = 0;
    for (let i = 0; i < 1500 && stable < 10; i++) {
      grab();
      if (total && seen.size >= total) break;
      const rows = document.querySelectorAll(ROW);
      if (rows.length) rows[rows.length - 1].scrollIntoView({ block: "start" });
      else sc.scrollTop += sc.clientHeight * 0.8;
      await sleep(400);
      stable = seen.size === last ? stable + 1 : 0;
      last = seen.size;
    }
    grab();
    post({ name: listName(), total, tracks: [...seen.values()].sort((a, b) => a.idx - b.idx) });
  }

  run().catch((e) => post({ error: String((e && e.message) || e) }));
})();
