// SoundCloud: кнопка ↓ в ряду «Like / Repost / Share / Copy Link» — у страницы трека/плейлиста,
// у каждого трека в списках и в ленте. Звук программа берёт прямо с SoundCloud.

(() => {
  const clean = (href) => {
    try {
      const u = new URL(href, location.origin);
      return u.origin + u.pathname;
    } catch {
      return href;
    }
  };

  /** Ссылка для группы кнопок: трек в списке, элемент ленты или сама страница. */
  function urlFor(group) {
    const item = group.closest(".trackList__item, .soundList__item, .searchList__item, .compactTrackList__item, .systemPlaylistTrackList__item");
    if (item) {
      const a =
        item.querySelector("a.trackItem__trackTitle, a.soundTitle__title, a.compactTrackListItem__trackTitle") ||
        item.querySelector("a[href*='/'][href]:not([href='']):not([href^='http'])");
      if (a) return clean(a.getAttribute("href"));
    }
    return clean(location.href);
  }

  function place() {
    document.querySelectorAll(".soundActions .sc-button-group").forEach((g) => {
      if (g.querySelector(".vgd-sc")) return;
      const size = g.classList.contains("sc-button-group-medium") ? "sc-button-medium" : "sc-button-small";
      const isSet = /\/sets\//.test(urlFor(g));
      const b = VGD.button(`vgd-sc sc-button sc-button-secondary sc-button-icon ${size}`, isSet ? "Скачать плейлист в Very Good Downloader" : "Скачать в Very Good Downloader", (btn) =>
        VGD.sendUrl(urlFor(g), btn, isSet ? "плейлист" : "трек")
      );
      const copy = g.querySelector(".sc-button-copylink, .sc-button-share");
      if (copy) copy.insertAdjacentElement("afterend", b);
      else g.appendChild(b);
    });
  }

  VGD.every(place);
})();
