// Яндекс Музыка: кнопка ↓ встраивается в ряды кнопок самого сайта и берёт их классы — тот же размер,
// фон и наведение. В шапке: «Воспроизведение · Нравится · ↓ · …». У трека: ↓ перед «…».
// Список целиком программа берёт сама через открытый API Яндекса — ограничения в 100 нет.

(() => {
  const MENU = 'button[aria-label="Контекстное меню"], button[aria-label="Context menu"]';
  const TRACK_LINK = "a[href*='trackId='], a[href*='/track/']";

  const clean = (href) => {
    try {
      const u = new URL(href, location.origin);
      const track = u.searchParams.get("trackId");
      if (track && /\/album\/\d+/.test(u.pathname)) return `${u.origin}${u.pathname.replace(/\/+$/, "")}/track/${track}`;
      return u.origin + u.pathname;
    } catch {
      return href;
    }
  };

  const pageKind = () => {
    const p = location.pathname;
    if (/\/track\/\d+/.test(p) || new URLSearchParams(location.search).get("trackId")) return "трек";
    if (/^\/album\//.test(p)) return "альбом";
    if (/^\/artist\//.test(p)) return "исполнитель";
    if (/playlists?\//.test(p)) return "плейлист";
    return null;
  };

  /** Кнопка в стиле соседней кнопки сайта (копируем её классы и размер значка). */
  function like(ref, cls, title, onClick) {
    const b = VGD.button(cls, title, onClick);
    b.className = `${ref.className} vgd-icon vgd-ym ${cls}`;
    const svg = ref.querySelector("svg");
    const size = svg ? parseFloat(getComputedStyle(svg).width) || 24 : 24;
    b.style.setProperty("--vgd-glyph", `${size}px`);
    return b;
  }

  function placeHeader() {
    const kind = pageKind();
    const play = document.querySelector(
      '[class*="CommonPageHeader_controls"] button[aria-label="Воспроизведение"], [class*="PageHeader"] [class*="playControl"]'
    );
    const row = play && play.parentElement;
    if (!kind || !row) return false;
    if (row.querySelector(".vgd-ym-head")) return true;
    const ref =
      row.querySelector('[class*="menuControl"] button') ||
      row.querySelector('button[aria-label="Нравится"]') ||
      [...row.querySelectorAll("button")].find((b) => b !== play) ||
      play;
    const title = `Скачать ${kind === "исполнитель" ? "все треки исполнителя" : kind} в Very Good Downloader`;
    const b = like(ref, "vgd-ym-head", title, (btn) => VGD.sendUrl(clean(location.href), btn, kind));
    // Порядок: Слушать · Нравится · … · ↓
    const menu = row.querySelector('[class*="menuControl"]') || [...row.children].find((c) => c.matches(MENU) || c.querySelector(MENU));
    // Последней в ряду, после «…».
    if (menu) menu.insertAdjacentElement("afterend", b);
    else row.appendChild(b);
    return true;
  }

  function placeRows() {
    document.querySelectorAll(MENU).forEach((menuBtn) => {
      if (menuBtn.closest('[class*="PageHeader"]')) return;
      // Строка трека — ближайший предок со ссылкой на трек.
      let row = menuBtn.parentElement;
      for (let i = 0; i < 8 && row && !row.querySelector(TRACK_LINK); i++) row = row.parentElement;
      if (!row || row.querySelector(".vgd-ym-row")) return;
      const href = row.querySelector(TRACK_LINK).getAttribute("href");
      // В один ряд с сердечком, перед ним (если сердечка нет — перед «…»).
      const heart = row.querySelector('button[aria-label*="Нравится"], button[aria-label*="нравится"], button[aria-label*="Like"]');
      const ref = heart || menuBtn;
      const b = like(ref, "vgd-ym-row", "Скачать трек в Very Good Downloader", (btn) => VGD.sendUrl(clean(href), btn, "трек"));
      // Если кнопка обёрнута в свой блок — ставим рядом с блоком, чтобы не сломать его раскладку.
      const target = ref.parentElement !== row && ref.parentElement.children.length === 1 ? ref.parentElement : ref;
      target.insertAdjacentElement("beforebegin", b);
    });
  }

  function place() {
    const ok = placeHeader();
    placeRows();
    // Шапки не нашлось (разметка поменялась) — круглая кнопка в углу.
    let f = document.getElementById("vgd-ym-float");
    const need = pageKind() && !ok;
    if (need && !f) {
      f = VGD.button("vgd-floating-icon", "Скачать в Very Good Downloader", (btn) => VGD.sendUrl(clean(location.href), btn, pageKind() || "ссылка"));
      f.id = "vgd-ym-float";
      document.body.appendChild(f);
    } else if (!need && f) {
      f.remove();
    }
  }

  VGD.every(place);
})();
