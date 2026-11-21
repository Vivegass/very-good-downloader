// Списки треков со страниц сервисов, у которых программа не берёт звук (Deezer, Tidal, Qobuz, Звук,
// Beatport, Amazon Music): кнопка «↓ список» внизу слева. Страница открыта у пользователя — с его входом,
// поэтому видны и свои плейлисты, и «Любимые треки». Программа получает только названия, а звук
// ищет в своих источниках (VK, Яндекс, YouTube Music, SoundCloud…), как для Spotify.
//
// Разметка у сайтов разная, поэтому строки ищутся по общему признаку: длительность «3:45» и рядом
// ссылка на трек или исполнителя. Длинные списки виртуальные — прокручиваем до конца, как на Spotify.

(() => {
  const SERVICES = [
    [/(^|\.)deezer\.com$/, "Deezer"],
    [/(^|\.)tidal\.com$/, "Tidal"],
    [/(^|\.)qobuz\.com$/, "Qobuz"],
    [/(^|\.)zvuk\.com$/, "Звук"],
    [/(^|\.)beatport\.com$/, "Beatport"],
    [/(^|\.)music\.amazon\./, "Amazon Music"],
  ];
  const service = (SERVICES.find(([re]) => re.test(location.hostname)) || [, "Музыка"])[1];
  const DUR = /^\d{1,2}:\d{2}(:\d{2})?$/;
  const TITLE_SEL = [
    '[data-test="table-cell-title"]',
    '[data-testid="title"]',
    '[data-test="track-title"]',
    '[data-testid*="track-title" i]',
    '[class*="track__title" i]',
    '[class*="TrackTitle" i]',
    'a[href*="/track/"]',
  ].join(", ");

  const text = (el) => (el ? el.textContent.replace(/\s+/g, " ").trim() : "");

  /** Длительности на странице (листья с текстом «3:45»), кроме плеера внизу. */
  function durations() {
    const out = [];
    const walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const el = n.parentElement;
      if (!el || !DUR.test(n.textContent.trim()) || el.closest("footer, [class*='player' i], [data-test*='player' i], .vgd-list")) continue;
      out.push(el);
    }
    return out;
  }

  /** Строка трека: ближайший предок длительности, где есть ссылка на трек или исполнителя, но нет второй длительности. */
  function rows() {
    const durs = durations();
    const found = new Set();
    for (const d of durs) {
      const byRole = d.closest('[role="row"]');
      if (byRole) {
        found.add(byRole);
        continue;
      }
      let r = d;
      let best = null;
      for (let i = 0; i < 12 && r.parentElement && r.parentElement !== document.body; i++) {
        const p = r.parentElement;
        if (durs.some((x) => x !== d && p.contains(x))) break;
        r = p;
        if (r.querySelector('a[href*="/track/"], a[href*="/artist/"]')) best = r;
      }
      if (best) found.add(best);
    }
    return [...found];
  }

  /** Исполнитель страницы (у альбома в строках его часто нет). */
  function pageArtist(rowsList) {
    const a = [...document.querySelectorAll('a[href*="/artist/"]')].find((x) => !rowsList.some((r) => r.contains(x)) && text(x));
    return text(a);
  }

  function rowTrack(r, fallbackArtist, i) {
    const t = r.querySelector(TITLE_SEL);
    const title = text(t).replace(/^\d+\.\s*/, "");
    if (!title) return null;
    const artists = [...new Set([...r.querySelectorAll('a[href*="/artist/"]').values()].map(text).filter(Boolean))];
    const album = text(r.querySelector('a[href*="/album/"], a[href*="/release/"]'));
    const durEl = [...r.querySelectorAll("*")].find((e) => !e.children.length && DUR.test(e.textContent.trim()));
    const link = r.querySelector('a[href*="/track/"]');
    const id =
      r.getAttribute("data-track-id") ||
      r.getAttribute("data-entity-id") ||
      (link ? (link.getAttribute("href").match(/\/track\/(?:[^/]+\/)?(\d+)/) || [])[1] : "") ||
      "";
    const img = r.querySelector("img[src^='https']");
    let cover = img ? img.src : "";
    if (/dzcdn\.net/.test(cover)) cover = cover.replace(/\/\d+x\d+-/, "/500x500-");
    else cover = "";
    const num = r.getAttribute("aria-rowindex") || r.getAttribute("data-index");
    return {
      idx: +(num || i),
      numbered: num !== null && num !== "",
      id,
      title,
      artist: artists.join(", ") || fallbackArtist,
      album: album && album !== title ? album : "",
      duration: VGD.toSeconds(durEl ? durEl.textContent : ""),
      cover: cover || undefined,
    };
  }

  function grab(seen) {
    const list = rows();
    const fa = pageArtist(list);
    list.forEach((r, i) => {
      const t = rowTrack(r, fa, i);
      if (!t) return;
      const k = (t.id || "") + "|" + t.artist.toLowerCase() + "|" + t.title.toLowerCase();
      if (!seen.has(k)) seen.set(k, t);
    });
    return list;
  }

  async function collectAll() {
    const seen = new Map();
    const start = window.scrollY;
    let last = -1;
    let stable = 0;
    for (let i = 0; i < 1500 && stable < 6; i++) {
      const list = grab(seen);
      VGD.toast(`собираю треки: ${seen.size}…`, "wait");
      if (list.length) list[list.length - 1].scrollIntoView({ block: "start" });
      else window.scrollBy(0, innerHeight * 0.8);
      await new Promise((r) => setTimeout(r, 400));
      stable = seen.size === last ? stable + 1 : 0;
      last = seen.size;
    }
    grab(seen);
    window.scrollTo(0, start);
    // По порядку на странице: у виртуальных списков есть номер строки (aria-rowindex / data-index), иначе — как собрали.
    const all = [...seen.values()];
    const numbered = all.every((t) => t.numbered);
    return numbered ? all.sort((a, b) => a.idx - b.idx) : all;
  }

  function listName() {
    // У Deezer нет h1, а заголовок вкладки — играющий трек; тогда og:title.
    const h = document.querySelector("main h1, h1");
    const og = document.querySelector('meta[property="og:title"]');
    return text(h) || (og && og.content.trim()) || document.title.replace(/\s*[-|—].*$/, "").trim() || service;
  }

  let btn = null;
  VGD.every(() => {
    const n = rows().length;
    if (!n) {
      if (btn) btn.style.display = "none";
      return;
    }
    if (!btn) {
      btn = document.createElement("button");
      btn.type = "button";
      btn.className = "vgd-list";
      btn.style.cssText =
        "position:fixed;left:20px;bottom:96px;z-index:2147483646;padding:10px 16px;border:0;border-radius:22px;cursor:pointer;" +
        "font:600 14px/1 system-ui,sans-serif;color:#fff;background:#1f6feb;box-shadow:0 4px 16px rgba(0,0,0,.3)";
      btn.title = "Отправить список треков в Very Good Downloader (звук программа найдёт сама)";
      btn.addEventListener("click", async (e) => {
        e.preventDefault();
        e.stopPropagation();
        btn.disabled = true;
        const tracks = await collectAll();
        btn.disabled = false;
        if (!tracks.length) return VGD.toast("не нашёл треков на странице", "err");
        VGD.sendTracks(listName(), service, tracks.map(({ idx, numbered, ...t }) => t), btn);
      });
      document.documentElement.appendChild(btn);
    }
    btn.style.display = "";
    btn.textContent = n === 1 ? "↓ трек в VGD" : "↓ список в VGD";
  });
})();
