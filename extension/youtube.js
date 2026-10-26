// YouTube: круглая кнопка-значок в ряду «Нравится / Поделиться» под видео,
// в боковой колонке Shorts и (для плейлистов и каналов) — в правом нижнем углу.

(() => {
  const ID = "vgd-yt";

  /** Чистая ссылка: видео без «микса», времени и меток. */
  function videoUrl() {
    const u = new URL(location.href);
    if (u.pathname === "/watch" && u.searchParams.get("v")) {
      return "https://www.youtube.com/watch?v=" + u.searchParams.get("v");
    }
    if (u.pathname.startsWith("/shorts/")) {
      return "https://www.youtube.com/shorts/" + u.pathname.split("/")[2];
    }
    return location.href;
  }

  function kind() {
    const p = location.pathname;
    if (p === "/watch") return "watch";
    if (p.startsWith("/shorts/")) return "shorts";
    if (p === "/playlist") return "playlist";
    if (/^\/(@|channel\/|c\/|user\/)/.test(p)) return "channel";
    return null;
  }

  const TITLES = {
    watch: "Скачать видео в Very Good Downloader",
    shorts: "Скачать Short в Very Good Downloader",
    playlist: "Скачать плейлист в Very Good Downloader",
    channel: "Скачать канал в Very Good Downloader",
  };

  function make(k, cls) {
    const b = VGD.button(cls, TITLES[k], (btn) => VGD.sendUrl(videoUrl(), btn, k === "playlist" ? "плейлист" : k === "channel" ? "канал" : "видео"));
    b.id = ID;
    b.dataset.kind = k;
    return b;
  }

  function place() {
    const k = kind();
    let b = document.getElementById(ID);
    if (!k) {
      if (b) b.remove();
      return;
    }
    if (b && b.dataset.kind !== k) {
      b.remove();
      b = null;
    }
    let anchor = null;
    let cls = "vgd-yt-inline";
    if (k === "watch") {
      anchor = document.querySelector("ytd-watch-metadata #top-level-buttons-computed");
    } else if (k === "shorts") {
      anchor = document.querySelector("ytd-reel-video-renderer[is-active] #actions, ytd-shorts [is-active] #actions");
      cls = "vgd-yt-shorts";
    }
    if (anchor) {
      if (!b || b.parentElement !== anchor) {
        if (b) b.remove();
        b = make(k, cls);
        if (k === "shorts") anchor.prepend(b);
        else anchor.appendChild(b);
      }
      return;
    }
    if (!b) {
      b = make(k, "vgd-floating-icon");
      document.body.appendChild(b);
    }
  }

  VGD.every(place);
  document.addEventListener("yt-navigate-finish", place);
})();
