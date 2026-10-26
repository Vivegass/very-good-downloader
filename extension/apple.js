// Apple Music: кнопка ↓ в ряду кнопок шапки альбома/плейлиста («Воспроизвести», «Перемешать»…)
// и у каждого трека. Список и звук программа находит сама (Apple звук не отдаёт: он зашифрован).

(() => {
  const clean = (href) => {
    try {
      const u = new URL(href, location.origin);
      const i = u.searchParams.get("i");
      return u.origin + u.pathname + (i ? `?i=${i}` : "");
    } catch {
      return href;
    }
  };

  const kind = () => {
    const p = location.pathname;
    if (/\/song\//.test(p) || new URLSearchParams(location.search).get("i")) return "трек";
    if (/\/album\//.test(p)) return "альбом";
    if (/\/playlist\//.test(p)) return "плейлист";
    if (/\/artist\//.test(p)) return "исполнитель";
    return null;
  };

  function place() {
    const k = kind();
    const actions = document.querySelector(".container-detail-header .primary-actions, .primary-actions");
    if (k && actions && !actions.querySelector(".vgd-am-head")) {
      const wrap = document.createElement("div");
      wrap.className = "primary-actions__button vgd-am-wrap";
      const b = VGD.button("vgd-am-head", `Скачать ${k} в Very Good Downloader`, (btn) => VGD.sendUrl(clean(location.href), btn, k));
      wrap.appendChild(b);
      actions.appendChild(wrap);
    }
    document.querySelectorAll(".songs-list-row").forEach((row) => {
      if (row.querySelector(".vgd-am-row")) return;
      const a = row.querySelector("a[href*='/song/'], a[href*='?i=']");
      if (!a) return;
      const b = VGD.button("vgd-am-row", "Скачать трек в Very Good Downloader", (btn) => VGD.sendUrl(clean(a.getAttribute("href")), btn, "трек"));
      const preview = row.querySelector(".songs-list-row__preview-button, [class*='context-menu'], [class*='more']");
      if (preview) preview.insertAdjacentElement("beforebegin", b);
      else row.appendChild(b);
    });
  }

  VGD.every(place);
})();
