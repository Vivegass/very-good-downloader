// Spotify: родная кнопка «Скачать» (↓) на странице плейлиста/альбома теперь отправляет весь список
// в программу. Если её нет (не вошли в аккаунт) — рядом с «Добавить в медиатеку» появляется своя.
// У каждого трека — маленькая кнопка ↓ при наведении.
// Список собираем со страницы: программе Spotify отдаёт только первые 100 треков.

(() => {
  const isList = () => /^\/(intl-[a-z-]+\/)?(playlist|album)\//.test(location.pathname);
  const isTrack = () => /^\/(intl-[a-z-]+\/)?track\//.test(location.pathname);
  const isArtist = () => /^\/(intl-[a-z-]+\/)?artist\//.test(location.pathname);

  /** Сколько треков по заголовку («201 трек», «45 songs»). Без \b: он не работает с кириллицей. */
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
    return {
      idx: row ? +row.getAttribute("aria-rowindex") : 0,
      id,
      title: t.textContent.trim(),
      artist: artists.join(", "),
      album: album ? album.textContent.trim() : "",
      duration: VGD.toSeconds(dur),
    };
  }

  function scroller() {
    let el = document.querySelector('[data-testid="tracklist-row"]');
    while (el && el !== document.body) {
      const s = getComputedStyle(el);
      if ((s.overflowY === "auto" || s.overflowY === "scroll") && el.scrollHeight > el.clientHeight + 20) return el;
      el = el.parentElement;
    }
    return document.scrollingElement;
  }

  /** Прокрутить список до конца и собрать все треки (список виртуальный — видны только ~25 строк). */
  async function collectAll() {
    const seen = new Map();
    const total = totalFromHeader();
    const sc = scroller();
    const start = sc.scrollTop;
    const grab = () =>
      document.querySelectorAll('[data-testid="tracklist-row"]').forEach((r) => {
        const t = rowTrack(r);
        if (t && !seen.has(t.id)) seen.set(t.id, t);
      });
    let last = -1;
    let stable = 0;
    for (let i = 0; i < 1500 && stable < 8; i++) {
      grab();
      VGD.toast(total ? `собираю треки: ${seen.size} из ${total}…` : `собираю треки: ${seen.size}…`, "wait");
      if (total && seen.size >= total) break;
      const rows = document.querySelectorAll('[data-testid="tracklist-row"]');
      if (rows.length) rows[rows.length - 1].scrollIntoView({ block: "start" });
      else sc.scrollTop += sc.clientHeight * 0.8;
      await new Promise((r) => setTimeout(r, 350));
      stable = seen.size === last ? stable + 1 : 0;
      last = seen.size;
    }
    grab();
    sc.scrollTop = start;
    return { tracks: [...seen.values()].sort((a, b) => a.idx - b.idx), total };
  }

  function listName() {
    const h = document.querySelector('[data-testid="entityTitle"], main h1');
    return (h && h.textContent.trim()) || document.title.replace(/\s*[-|].*$/, "").trim() || "Spotify";
  }

  async function downloadList(btn) {
    if (btn) btn.classList.add("vgd-busy");
    const { tracks, total } = await collectAll();
    if (btn) btn.classList.remove("vgd-busy");
    if (!tracks.length) return VGD.toast("не нашёл треков на странице", "err");
    if (total && tracks.length < total) VGD.toast(`собрал ${tracks.length} из ${total} — отправляю`, "wait");
    VGD.sendTracks(listName(), "Spotify", tracks, btn);
  }

  /** Родная кнопка «Скачать» Spotify: перехватываем щелчок (у бесплатных аккаунтов она всё равно не работает). */
  function hookNative(bar) {
    const native = [...bar.querySelectorAll("button")].find((b) =>
      /скачать|загрузить|download/i.test(b.getAttribute("aria-label") || "")
    );
    if (!native || native.dataset.vgd) return !!native;
    native.dataset.vgd = "1";
    native.title = "Скачать в Very Good Downloader";
    native.addEventListener(
      "click",
      (e) => {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        downloadList(native);
      },
      true
    );
    return true;
  }

  /** Кнопка ↓ в плеере внизу: скачать то, что играет сейчас. */
  function placePlayer() {
    const w = document.querySelector('[data-testid="now-playing-widget"]');
    if (!w || w.querySelector(".vgd-sp-player")) return;
    const b = VGD.button("vgd-sp-row vgd-sp-player", "Скачать этот трек в Very Good Downloader", (btn) => {
      const title = w.querySelector('[data-testid="context-item-link"]')?.textContent.trim();
      const artist = [...w.querySelectorAll('[data-testid="context-item-info-artist"]')].map((a) => a.textContent.trim()).join(", ");
      const href = w.querySelector('a[href*="/track/"]')?.getAttribute("href") || "";
      const id = href.split("/track/")[1]?.split(/[?#]/)[0];
      if (!title) return VGD.toast("не понял, какой трек играет", "err");
      VGD.sendTracks(title, "Spotify", [{ id, title, artist }], btn);
    });
    const add = w.querySelector('[data-testid="add-button"], button[aria-label*="медиатек"], button[aria-label*="Library"]');
    if (add) add.insertAdjacentElement("afterend", b);
    else w.appendChild(b);
  }

  function place() {
    placePlayer();
    // Плейлист / альбом: кнопка в строке действий.
    const bar = document.querySelector('[data-testid="action-bar-row"]');
    if (bar && isArtist() && !bar.querySelector(".vgd-sp-bar")) {
      // Исполнитель: вся дискография (список альбомов программа берёт из каталога Deezer).
      const b = VGD.button("vgd-sp-bar", "Скачать всю дискографию в Very Good Downloader", (btn) =>
        VGD.sendUrl(location.href.split("?")[0], btn, "дискография")
      );
      const after = bar.querySelector('[data-testid="add-button"], button:not([data-testid="play-button"])');
      if (after) after.insertAdjacentElement("afterend", b);
      else bar.appendChild(b);
    }
    if (bar && (isList() || isTrack())) {
      if (isList() && hookNative(bar)) {
        bar.querySelector(".vgd-sp-bar")?.remove();
      } else if (!bar.querySelector(".vgd-sp-bar")) {
        const b = VGD.button("vgd-sp-bar", isTrack() ? "Скачать трек в Very Good Downloader" : "Скачать в Very Good Downloader", (btn) => {
          if (isTrack()) VGD.sendUrl(location.href.split("?")[0], btn, "трек");
          else downloadList(btn);
        });
        const after = bar.querySelector('[data-testid="add-button"]') || bar.querySelector("button:not([data-testid='play-button'])");
        if (after && after.parentElement) after.insertAdjacentElement("afterend", b);
        else bar.appendChild(b);
      }
    }
    // Кнопка у каждого трека (видна при наведении на строку).
    document.querySelectorAll('[data-testid="tracklist-row"]').forEach((r) => {
      if (r.querySelector(".vgd-sp-row")) return;
      const more = r.querySelector('[data-testid="more-button"]');
      if (!more) return;
      const b = VGD.button("vgd-sp-row", "Скачать трек в Very Good Downloader", (btn) => {
        const t = rowTrack(r);
        if (t) VGD.sendTracks(t.title, "Spotify", [t], btn);
      });
      more.insertAdjacentElement("beforebegin", b);
    });
  }

  VGD.every(place);
})();
