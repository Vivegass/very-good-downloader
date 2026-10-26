// VK Музыка. Кнопки стоят там же, где у VK Music Saver: в ряду действий каждого трека
// и в шапке плейлиста/альбома рядом с «Слушать». VK отдаёт треки только вошедшим, поэтому
// перед отправкой vk-main.js получает на странице ссылки на звук (HLS) — программа качает
// звук прямо из VK. Трекам без ссылки (заблокирован, нет прав) программа ищет замену на
// YouTube Music / SoundCloud / YouTube.

(() => {
  // Строки треков: новая разметка (React) и старая (.audio_row).
  const ROWS = '[class*="AudioRow__root"], [data-testentitytag="audio"], .audio_row';
  // Блок с кнопками в строке трека (♥, «+», «…»).
  const ROW_ACTIONS = '[class*="AudioRow__actions--"], [data-testid="audiorow-actions"], .audio_row__actions';
  // Кнопки в шапке плейлиста / альбома / окна с плейлистом.
  const HEAD_ACTIONS = [
    '[class*="AudioListHeader__actions--"]',
    '[class*="AudioListBoxHeader__actions--"]',
    '[class*="AudioListModalHeader__actions--"]',
    '[data-testid="MusicPlaylistPage_Header_Actions"]',
    '[data-testid="audiolistboxheader-actions"]',
    ".AudioPlaylistSnippet__actions",
    ".audio_pl_snippet__actions",
  ].join(",");

  function decode(html) {
    const t = document.createElement("textarea");
    t.innerHTML = html || "";
    return t.value.trim();
  }

  /** Трек из строки: свойства React (vk-main.js), data-audio старой разметки или текст строки. */
  function rowTrack(row) {
    if (row.dataset.vgdAudio) {
      try {
        const a = JSON.parse(row.dataset.vgdAudio);
        if (a.title) return a;
      } catch {}
    }
    const raw = row.getAttribute("data-audio");
    if (raw) {
      try {
        const a = JSON.parse(raw); // [id, owner, url, title, performer, duration, …, 13: хеши, …, 24: access key]
        const h = String(a[13] || "").split("/");
        return {
          id: a[1] + "_" + a[0],
          title: decode(a[3]),
          artist: decode(a[4]),
          duration: +a[5] || null,
          accessKey: a[24] || undefined,
          actionHash: h[2] || undefined,
          urlHash: h[5] || undefined,
        };
      } catch {}
    }
    const q = (sel) => row.querySelector(sel)?.textContent.trim() || "";
    let title = q(".audio_row__title_inner") || q('[class*="AudioRow__title"]') || q('[class*="Title--"]');
    let artist = q(".audio_row__performers") || q('[class*="AudioRow__artists"]') || q('[class*="AudioRow__performers"]');
    let duration = VGD.toSeconds(q(".audio_row__duration") || q('[class*="AudioRow__duration"]') || q('[class*="uration"]'));
    if (!title) {
      // Запасной путь: строки текста «Название / Исполнитель / 3:45».
      const lines = row.innerText.split("\n").map((s) => s.trim()).filter((s) => s && s.length > 1);
      const words = lines.filter((s) => !/^\d{1,2}:\d{2}(:\d{2})?$/.test(s));
      title = words[0] || "";
      artist = artist || words[1] || "";
      duration = duration || VGD.toSeconds(lines.find((s) => /^\d{1,2}:\d{2}/.test(s)));
    }
    return title && title.length < 300 ? { title, artist, duration } : null;
  }

  const topRows = (scope) =>
    [...(scope || document).querySelectorAll(ROWS)].filter((el) => !(el.parentElement && el.parentElement.closest(ROWS)));

  /** Блок с треками именно этого плейлиста: ближайший предок шапки, в котором есть строки.
   *  (Иначе в окне плейлиста поверх «Моей музыки» собиралась бы вся музыка со страницы.) */
  function listScope(head) {
    let el = head;
    for (let i = 0; i < 12 && el; i++, el = el.parentElement) {
      const list = el.querySelector('[data-testid="audiolistitems-list"], [class*="AudioList__items"], [class*="AudioListItems"]');
      if (list) return list;
      if (el.querySelector(ROWS)) return el;
    }
    return document;
  }

  function collect(scope) {
    const out = [];
    const seen = new Set();
    const rows = topRows(scope);
    rows.forEach((el) => {
      const t = rowTrack(el);
      if (!t) return;
      const k = (t.id || t.artist + "|" + t.title).toLowerCase();
      if (!seen.has(k)) {
        seen.add(k);
        out.push(t);
      }
    });
    return out;
  }

  function playlistName(head) {
    const root = head?.closest('[class*="AudioListHeader"], [class*="AudioListBoxHeader"], [class*="AudioListModalHeader"], [data-testid*="Header"]');
    // Ближайший к шапке заголовок (в окне плейлиста h1 страницы — от того, что под окном).
    let near = null;
    for (let el = head?.parentElement, i = 0; el && i < 8 && !near; el = el.parentElement, i++) {
      near = [...el.querySelectorAll("h1, h2, h3, [class*='itle']")].find((x) => !x.closest(".vgd-vk-hb") && x.textContent.trim());
    }
    const h =
      root?.querySelector("h1, h2, [class*='title'], [class*='Title']") ||
      near ||
      document.querySelector(".AudioPlaylistSnippet__title, .audio_pl_snippet__info_title, h1");
    return (h && h.textContent.trim()) || document.title.replace(/\s*\|.*$/, "").trim() || "VK";
  }

  /** VK подгружает длинные плейлисты порциями — прокручиваем именно этот список до конца. */
  async function loadAll(scope) {
    let last = -1;
    let stable = 0;
    for (let i = 0; i < 200 && stable < 4; i++) {
      const rows = topRows(scope);
      if (rows.length) rows[rows.length - 1].scrollIntoView({ block: "end" });
      await new Promise((r) => setTimeout(r, 700));
      const n = collect(scope).length;
      VGD.toast(`собираю треки: ${n}…`, "wait");
      stable = n === last ? stable + 1 : 0;
      last = n;
    }
  }

  /** Запрос к vk-main.js (контекст страницы) через postMessage; ответ — {…данные, diag}. */
  function ask(type, answer, payload, timeout) {
    // vk-main.js работает только на полной версии сайта (m.vk.com — без него).
    if (/^m\./.test(location.hostname)) return Promise.resolve(null);
    const req = payload.req || Math.random().toString(36).slice(2);
    return new Promise((done) => {
      const timer = setTimeout(() => finish(null), timeout);
      function finish(data) {
        clearTimeout(timer);
        window.removeEventListener("message", onMsg);
        done(data);
      }
      function onMsg(e) {
        if (e.source === window && e.data && e.data.vgd === answer && e.data.req === req) finish(e.data);
      }
      window.addEventListener("message", onMsg);
      window.postMessage({ ...payload, vgd: type, req }, location.origin);
    });
  }

  /** Ссылки на звук: { "owner_id": {url, cover, album} } и диагностика. */
  async function resolveStreams(tracks) {
    const items = tracks.filter((t) => t.id).map(({ id, accessKey, actionHash, urlHash }) => ({ id, accessKey, actionHash, urlHash }));
    if (!items.length) return { found: {}, diag: { noIds: true } };
    const r = await ask("vk-resolve", "vk-resolved", { items }, 20000 + items.length * 150);
    return r ? { found: r.found || {}, diag: r.diag } : { found: {}, diag: { timeout: true } };
  }

  /** Отправить треки в программу вместе со ссылками на звук из VK. */
  async function sendVk(name, tracks, btn, diag) {
    btn?.classList.add("vgd-busy");
    VGD.toast(tracks.length === 1 ? "беру трек из VK…" : `беру ${tracks.length} треков из VK…`, "wait");
    const need = tracks.filter((t) => !t.url);
    const { found, diag: d2 } = need.length ? await resolveStreams(need) : { found: {} };
    btn?.classList.remove("vgd-busy");
    const out = tracks.map(({ accessKey, actionHash, urlHash, ...t }) => {
      const f = t.id && found[t.id];
      // свежая ссылка из API, иначе — та, что была в свойствах строки
      return f ? { ...t, url: f.url, cover: f.cover || t.cover, album: t.album || f.album } : t;
    });
    const n = out.filter((t) => t.url).length;
    if (n < out.length) console.info(`[Very Good Downloader] VK: ссылки на звук для ${n} из ${out.length}, остальные программа найдёт сама`);
    VGD.sendTracks(name, "VK", out, btn, { ...diag, resolve: d2, withUrl: n, total: out.length, page: location.pathname + location.search });
  }

  async function downloadPlaylist(btn, head) {
    // Сначала весь плейлист через API VK (все треки сразу, с ссылками на звук).
    btn.classList.add("vgd-busy");
    VGD.toast("получаю плейлист из VK…", "wait");
    const req = Math.random().toString(36).slice(2);
    head.setAttribute("data-vgd-req", req);
    const r = await ask("vk-playlist", "vk-playlist-done", { req }, 60000);
    head.removeAttribute("data-vgd-req");
    btn.classList.remove("vgd-busy");
    const diag = { playlist: r ? r.diag : { timeout: true } };
    if (r && r.list && r.list.tracks.length) {
      return sendVk(r.list.title || playlistName(head), r.list.tracks, btn, diag);
    }
    // Не вышло — как раньше: строки со страницы.
    btn.classList.add("vgd-busy");
    const y = window.scrollY;
    const scope = listScope(head);
    await loadAll(scope);
    window.scrollTo(0, y);
    btn.classList.remove("vgd-busy");
    const tracks = collect(scope);
    if (!tracks.length) return VGD.toast("не нашёл треков на странице", "err");
    sendVk(playlistName(head), tracks, btn, diag);
  }

  /** Кнопка как у VK Music Saver: span[role=button], щелчок не уходит в строку трека. */
  function vkButton(cls, title, onClick, text) {
    const b = VGD.button(cls, title, onClick);
    const span = document.createElement("span");
    span.className = b.className;
    span.title = b.title;
    span.setAttribute("role", "button");
    span.setAttribute("tabindex", "0");
    span.innerHTML = b.innerHTML + (text ? `<span class="vgd-vk-text">${text}</span>` : "");
    for (const ev of ["pointerdown", "mousedown", "mouseup", "dblclick"]) {
      span.addEventListener(ev, (e) => e.stopPropagation(), true);
    }
    span.addEventListener(
      "click",
      (e) => {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        onClick(span, e);
      },
      true
    );
    span.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onClick(span, e);
      }
    });
    return span;
  }

  const CLICKABLE = "button, [role='button'], a[href]";

  /** Ряд значков справа в строке трека (копировать / карандаш / дизлайк / ♪ / × / …):
   *  родитель, у которого больше всего кнопок-значков. */
  function iconGroup(row) {
    const counts = new Map();
    row.querySelectorAll(CLICKABLE).forEach((el) => {
      if (el.closest(".vgd-vk-row") || !el.querySelector("svg")) return;
      if (el.closest('[class*="cover"], [class*="Cover"], [class*="Thumb"], [class*="play"], [class*="Play"]')) return;
      const parent = el.parentElement;
      if (parent) counts.set(parent, (counts.get(parent) || 0) + 1);
    });
    let best = null;
    let max = 1;
    counts.forEach((n, el) => {
      if (n > max) {
        best = el;
        max = n;
      }
    });
    return best;
  }

  /** Длительность «3:45» в строке — запасное место для кнопки (перед ней, в той же строке). */
  function durationEl(row) {
    return [...row.querySelectorAll("span, div")].find((el) => !el.children.length && /^\d{1,2}:\d{2}$/.test(el.textContent.trim()));
  }

  function placeRow(row) {
    let b = row.querySelector(".vgd-vk-row");
    const group = row.querySelector(ROW_ACTIONS) || iconGroup(row);
    if (b && group && b.parentElement === group) return;
    if (b && !group) return;
    if (!b) {
      b = vkButton("vgd-vk-row", "Скачать трек в Very Good Downloader", (btn) => {
        const t = rowTrack(row);
        if (t) sendVk(t.title, [t], btn, { row: row.dataset.vgdAudio ? "react" : row.getAttribute("data-audio") ? "data-audio" : "text" });
        else VGD.toast("не понял, что это за трек", "err");
      });
    }
    if (group) {
      group.prepend(b); // перед первым значком ряда, как у VK Music Saver
    } else {
      const d = durationEl(row);
      if (d) d.insertAdjacentElement("beforebegin", b);
    }
  }

  /** Меню у кнопки шапки: плейлист в программу, обложка, список треков. */
  function headMenu(btn, head) {
    document.getElementById("vgd-vk-menu")?.remove();
    const m = document.createElement("div");
    m.id = "vgd-vk-menu";
    m.className = "vgd-vk-menu";
    const item = (text, fn) => {
      const it = document.createElement("div");
      it.className = "vgd-vk-menu-item";
      it.textContent = text;
      it.addEventListener("click", (e) => {
        e.stopPropagation();
        m.remove();
        fn();
      });
      m.appendChild(it);
    };
    item("Скачать плейлист", () => downloadPlaylist(btn, head));
    item("Скачать обложку", () => {
      const root = head.closest('[class*="AudioListHeader"], [class*="AudioListBoxHeader"], [class*="AudioListModalHeader"], [data-testid*="Header"]') || head.parentElement;
      const img = root && [...root.querySelectorAll("img")].sort((x, y) => y.naturalWidth - x.naturalWidth)[0];
      const bg = root && [...root.querySelectorAll("[style*='background-image']")].map((el) => /url\(["']?([^"')]+)/.exec(el.style.backgroundImage)?.[1]).find(Boolean);
      const src = img?.src || bg;
      if (!src) return VGD.toast("обложку не нашёл", "err");
      chrome.runtime.sendMessage({ type: "vgd-save-file", url: src, name: `${playlistName(head)}.jpg` }, (r) =>
        VGD.toast(r && r.ok ? "обложка сохранена в «Загрузки»" : "не удалось сохранить обложку", r && r.ok ? "ok" : "err")
      );
    });
    item("Скачать список треков (.txt)", async () => {
      const scope = listScope(head);
      await loadAll(scope);
      const tracks = collect(scope);
      const text = tracks.map((t, i) => `${i + 1}. ${t.artist} — ${t.title}`).join("\r\n");
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob(["\ufeff" + text], { type: "text/plain;charset=utf-8" }));
      a.download = `${playlistName(head)}.txt`;
      a.click();
      VGD.toast(`список: ${tracks.length} треков`, "ok");
    });
    const r = btn.getBoundingClientRect();
    m.style.top = `${r.bottom + window.scrollY + 6}px`;
    m.style.left = `${Math.max(8, r.right + window.scrollX - 240)}px`;
    document.body.appendChild(m);
    setTimeout(() => document.addEventListener("click", () => m.remove(), { once: true }), 0);
  }

  /** Ряд кнопок шапки (▶ ⇄ ✓ ♪): родитель первой кнопки внутри блока действий. */
  function headRow(head) {
    const first = [...head.querySelectorAll(CLICKABLE)].find((el) => !el.closest(".vgd-vk-hb"));
    return first ? first.parentElement : head;
  }

  function placeHead(head) {
    if (head.querySelector(".vgd-vk-hb")) return;
    const row = headRow(head);
    const siblings = [...row.querySelectorAll(":scope > button, :scope > [role='button'], :scope > a")];
    const ref = siblings[siblings.length - 1];
    const b = vkButton("vgd-vk-head vgd-vk-hb", "Скачать в Very Good Downloader", (btn) => headMenu(btn, head));
    if (ref) {
      // Та же белая круглая кнопка, что и соседние: берём их классы, внутри — наша стрелка.
      b.className = `${ref.className} vgd-vk-hb vgd-vk-native`;
      const svg = ref.querySelector("svg");
      const size = svg ? parseFloat(getComputedStyle(svg).width) || 20 : 20;
      b.style.setProperty("--vgd-glyph", `${size}px`);
    }
    row.appendChild(b);
  }

  /** Страница исполнителя: «Дискография» рядом с «Слушать». */
  function placeArtist() {
    if (!/\/artist\//.test(location.pathname) || document.querySelector(".vgd-vk-artist")) return;
    const listen = [...document.querySelectorAll("button, [role='button'], a")].find((el) => /^(слушать|listen)$/i.test(el.textContent.trim()));
    if (!listen) return;
    const name = (document.querySelector("h1, [class*='ArtistHeader'] [class*='itle']")?.textContent || "").trim();
    const b = vkButton(
      "vgd-vk-head vgd-vk-artist",
      "Скачать всю дискографию в Very Good Downloader",
      (btn) => {
        if (!name) return VGD.toast("не понял имя исполнителя", "err");
        VGD.sendUrl(`https://www.deezer.com/search/${encodeURIComponent(name)}#vgd-discography`, btn, `дискография «${name}»`);
      },
      "Дискография"
    );
    listen.insertAdjacentElement("afterend", b);
  }

  function place() {
    topRows().forEach(placeRow);
    document.querySelectorAll(HEAD_ACTIONS).forEach(placeHead);
    placeArtist();
  }

  VGD.every(place);
})();
