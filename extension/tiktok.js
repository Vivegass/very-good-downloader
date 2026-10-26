// TikTok: кнопка ↓ в колонке «лайк / комментарии / поделиться» у каждого видео в ленте и на странице видео,
// а на странице профиля — круглая кнопка в углу (скачать все видео автора).
// Программа качает без водяного знака и выбирает H.264 (HEVC многие программы не открывают).

(() => {
  const clean = (href) => {
    try {
      const u = new URL(href, location.origin);
      return u.origin + u.pathname;
    } catch {
      return href;
    }
  };

  const isVideoPage = () => /\/@[^/]+\/(video|photo)\/\d+/.test(location.pathname);
  const isProfile = () => /^\/@[^/]+\/?$/.test(location.pathname);

  /** Ссылка на видео для кнопки «поделиться»: ближайшая карточка со ссылкой или сама страница. */
  function videoUrl(el) {
    let node = el;
    for (let i = 0; i < 12 && node; i++, node = node.parentElement) {
      const a = node.querySelector && node.querySelector('a[href*="/video/"], a[href*="/photo/"]');
      if (a) return clean(a.href);
    }
    return isVideoPage() ? clean(location.href) : null;
  }

  function place() {
    // Колонка действий у видео: ставим ↓ над «поделиться».
    document.querySelectorAll('[data-e2e="share-icon"], [data-e2e="video-share-icon"]').forEach((share) => {
      const btn = share.closest("button") || share;
      const holder = btn.parentElement;
      if (!holder || holder.parentElement?.querySelector(":scope > .vgd-tt")) return;
      const b = VGD.button("vgd-tt", "Скачать видео в Very Good Downloader", (x) => {
        const url = videoUrl(btn);
        if (url) VGD.sendUrl(url, x, "видео");
        else VGD.toast("не понял, какое это видео", "err");
      });
      holder.insertAdjacentElement("beforebegin", b);
    });
    // Страница видео без колонки (другая разметка) или профиль — круглая кнопка в углу.
    let f = document.getElementById("vgd-tt-float");
    const need = (isVideoPage() && !document.querySelector(".vgd-tt")) || isProfile();
    if (need && !f) {
      f = VGD.button("vgd-floating-icon", isProfile() ? "Скачать все видео автора в Very Good Downloader" : "Скачать видео в Very Good Downloader", (x) =>
        VGD.sendUrl(clean(location.href), x, isProfile() ? "профиль" : "видео")
      );
      f.id = "vgd-tt-float";
      document.body.appendChild(f);
    } else if (!need && f) {
      f.remove();
    } else if (f) {
      f.title = isProfile() ? "Скачать все видео автора в Very Good Downloader" : "Скачать видео в Very Good Downloader";
    }
  }

  VGD.every(place);
})();
