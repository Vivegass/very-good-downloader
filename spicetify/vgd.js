// NAME: Very Good Downloader
// AUTHOR: Very Good Downloader
// DESCRIPTION: Кнопки «скачать» в приложении Spotify: трек, плейлист любой длины, альбом, исполнитель,
// «Любимые треки», текущий трек. Всё уходит в очередь программы Very Good Downloader.
//
// Расширение Spicetify. Список треков собирается внутри клиента (GraphQL / Platform API), поэтому
// ограничения «первые 100 треков» нет. Связь с программой — http://127.0.0.1:38517 (как у расширения
// браузера); если программа закрыта — запускаем её ссылкой vgd:// и ждём.

(async function vgd() {
  while (!window.Spicetify?.showNotification || !Spicetify.Platform || !Spicetify.ContextMenu || !Spicetify.URI) {
    await new Promise((r) => setTimeout(r, 300));
  }
  const S = Spicetify;
  const APP = "http://127.0.0.1:38517";
  const NAME = "Скачать в Very Good Downloader";
  const PAGE = 50;
  const ARROW =
    '<svg role="img" height="16" width="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" ' +
    'stroke-linecap="round" stroke-linejoin="round"><path d="M12 4.5v10m0 0-4-4m4 4 4-4M5.5 19h13"/></svg>';

  const say = (text, err) => S.showNotification(text, !!err, err ? 6000 : 3500);
  const first = (...v) => v.find((x) => typeof x === "string" && x.trim())?.trim() || "";
  const num = (v) => (typeof v === "number" && isFinite(v) ? v : typeof v === "string" && v && isFinite(+v) ? +v : undefined);
  const idOf = (uri) => (uri || "").split(":").pop();
  const typeOf = (uri) => {
    try {
      return S.URI.fromString(uri).type;
    } catch {
      return "";
    }
  };
  const T = S.URI.Type;
  const isTrack = (u) => typeOf(u) === T.TRACK;
  const isPlaylist = (u) => [T.PLAYLIST, T.PLAYLIST_V2].includes(typeOf(u));
  const isAlbum = (u) => typeOf(u) === T.ALBUM;
  const isArtist = (u) => typeOf(u) === T.ARTIST;
  const isLiked = (u) => typeOf(u) === T.COLLECTION || /:collection(:tracks)?$/.test(u || "");
  const artistNames = (items) =>
    (Array.isArray(items) ? items : []).map((a) => first(a?.profile?.name, a?.name, a?.title)).filter(Boolean);
  const bestImage = (src) =>
    Array.isArray(src) && src.length ? [...src].sort((a, b) => (b?.width || b?.maxWidth || 0) - (a?.width || a?.maxWidth || 0))[0]?.url : undefined;

  /** Трек в формате списка программы (как у расширения браузера). */
  const track = (uri, title, artist, album, ms, cover, idx) =>
    title ? { id: idOf(uri), title, artist: artist || "", album: album || "", duration: ms ? Math.round(ms / 1000) : undefined, cover, idx } : null;

  // ---------- связь с программой ----------

  async function post(body) {
    const r = await fetch(APP + "/add", {
      method: "POST",
      headers: { "X-VGD": "1", "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!r.ok) throw new Error("HTTP " + r.status);
    return r.json();
  }

  async function alive() {
    try {
      const c = new AbortController();
      setTimeout(() => c.abort(), 1500);
      return (await fetch(APP + "/ping", { headers: { "X-VGD": "1" }, signal: c.signal })).ok;
    } catch {
      return false;
    }
  }

  /** Программа закрыта — запускаем её через протокол vgd:// и ждём до 20 с. */
  async function launch() {
    say("Открываю Very Good Downloader…");
    try {
      const f = document.createElement("iframe");
      f.style.display = "none";
      f.src = "vgd://add";
      document.body.appendChild(f);
      setTimeout(() => f.remove(), 5000);
    } catch {}
    try {
      window.open("vgd://add");
    } catch {}
    for (let i = 0; i < 40; i++) {
      await new Promise((r) => setTimeout(r, 500));
      if (await alive()) return true;
    }
    return false;
  }

  async function send(body, what) {
    try {
      if (!(await alive()) && !(await launch())) {
        say("Very Good Downloader не запущен — откройте программу и нажмите ещё раз", true);
        return;
      }
      await post(body);
      say(what + " — в очереди Very Good Downloader ✓");
    } catch (e) {
      say("Не удалось отправить в программу: " + (e?.message || e), true);
    }
  }

  const sendList = (name, tracks, what) => {
    const list = tracks.filter(Boolean);
    if (!list.length) return say("Не нашёл треков для скачивания", true);
    return send({ name, service: "Spotify", tracks: list }, `${what}: ${list.length} трек(ов)`);
  };

  // ---------- сведения о треках ----------

  const gql = (name, vars) => {
    const def = S.GraphQL?.Definitions?.[name];
    if (!def) return Promise.reject(new Error("нет " + name));
    return S.GraphQL.Request(def, { locale: S.Locale?.getLocale?.() || "", ...vars });
  };

  /** Один трек: из плеера (если играет), иначе GraphQL, иначе Web API клиента. */
  async function trackInfo(uri) {
    const it = S.Player?.data?.item;
    if (it?.uri === uri) {
      const m = it.metadata || {};
      return track(uri, first(it.name, m.title), artistNames(it.artists).join(", ") || m.artist_name, first(it.album?.name, m.album_title),
        it.duration?.milliseconds || num(m.duration), first(m.image_xlarge_url, m.image_large_url, m.image_url));
    }
    try {
      const r = await gql("getTrack", { uri });
      const t = r?.data?.trackUnion;
      if (t?.name) {
        return track(uri, t.name, artistNames(t.firstArtist?.items?.concat(t.otherArtists?.items || []) || t.artists?.items).join(", "),
          t.albumOfTrack?.name, t.duration?.totalMilliseconds, bestImage(t.albumOfTrack?.coverArt?.sources));
      }
    } catch {}
    const t = await S.CosmosAsync.get("https://api.spotify.com/v1/tracks/" + idOf(uri));
    return track(uri, t?.name, artistNames(t?.artists).join(", "), t?.album?.name, t?.duration_ms, bestImage(t?.album?.images));
  }

  async function tracksInfo(uris) {
    const out = [];
    // Web API: по 50 за раз — быстро для многих выделенных треков.
    for (let i = 0; i < uris.length; i += 50) {
      const part = uris.slice(i, i + 50);
      try {
        const r = await S.CosmosAsync.get("https://api.spotify.com/v1/tracks?ids=" + part.map(idOf).join(","));
        r.tracks.forEach((t, k) =>
          out.push(t && track(part[k], t.name, artistNames(t.artists).join(", "), t.album?.name, t.duration_ms, bestImage(t.album?.images))));
      } catch {
        for (const u of part) out.push(await trackInfo(u).catch(() => null));
      }
    }
    return out;
  }

  /** Плейлист целиком: GraphQL постранично → Platform PlaylistAPI → Web API. */
  async function playlist(uri) {
    try {
      const tracks = [];
      let name = "", offset = 0, total;
      while (total === undefined || offset < total) {
        const r = await gql("fetchPlaylistContents", { uri, offset, limit: PAGE });
        if (r?.errors) throw new Error("GraphQL");
        const c = r?.data?.playlistV2?.content;
        const items = c?.items || [];
        total = c?.totalCount ?? offset + items.length;
        for (const it of items) {
          const d = it?.itemV2?.data || it?.itemV3?.data;
          if (!d?.uri || !isTrack(d.uri)) continue;
          tracks.push(track(d.uri, first(d.name, d.identityTrait?.name),
            artistNames(d.artists?.items || d.identityTrait?.contributors?.items).join(", "),
            first(d.albumOfTrack?.name, d.identityTrait?.contentHierarchyParent?.identityTrait?.name),
            d.trackDuration?.totalMilliseconds, bestImage(d.albumOfTrack?.coverArt?.sources), tracks.length + 1));
        }
        if (!items.length) break;
        offset += items.length;
      }
      if (offset === 0) throw new Error("пусто");
      try {
        name = (await gql("fetchPlaylistMetadata", { uri }))?.data?.playlistV2?.name || "";
      } catch {}
      return { name: name || (await playlistName(uri)), tracks };
    } catch {}
    const api = S.Platform.PlaylistAPI;
    if (api?.getContents) {
      try {
        const meta = api.getMetadata ? await api.getMetadata(uri).catch(() => null) : null;
        const c = await api.getContents(uri, { limit: 100000, offset: 0 });
        const tracks = (c?.items || [])
          .filter((i) => isTrack(i.uri))
          .map((i, k) => track(i.uri, i.name, artistNames(i.artists).join(", "), i.album?.name, i.duration?.milliseconds, bestImage(i.album?.images), k + 1));
        if (tracks.length) return { name: meta?.name || (await playlistName(uri)), tracks };
      } catch {}
    }
    // Последний вариант — Web API от имени клиента (по 100, сколько угодно страниц).
    const tracks = [];
    let url = `https://api.spotify.com/v1/playlists/${idOf(uri)}/tracks?limit=100`;
    while (url) {
      const r = await S.CosmosAsync.get(url);
      for (const it of r?.items || []) {
        const t = it?.track;
        if (t?.uri && isTrack(t.uri)) tracks.push(track(t.uri, t.name, artistNames(t.artists).join(", "), t.album?.name, t.duration_ms, bestImage(t.album?.images), tracks.length + 1));
      }
      url = r?.next;
    }
    return { name: await playlistName(uri), tracks };
  }

  async function playlistName(uri) {
    try {
      return (await S.CosmosAsync.get(`https://api.spotify.com/v1/playlists/${idOf(uri)}?fields=name`))?.name || "Spotify";
    } catch {
      return document.querySelector("h1")?.textContent?.trim() || "Spotify";
    }
  }

  /** «Любимые треки»: LibraryAPI → Web API /me/tracks. */
  async function liked() {
    const lib = S.Platform.LibraryAPI;
    if (lib?.getTracks) {
      try {
        const r = await lib.getTracks({ limit: -1, offset: 0, sort: { field: "ADDED_AT", order: "DESC" } });
        const items = r?.items || [];
        if (items.length) {
          return items
            .filter((i) => isTrack(i.uri))
            .map((i, k) => track(i.uri, i.name, artistNames(i.artists).join(", "), i.album?.name, i.duration?.milliseconds, bestImage(i.album?.images), k + 1));
        }
      } catch {}
    }
    const tracks = [];
    let url = "https://api.spotify.com/v1/me/tracks?limit=50";
    while (url) {
      const r = await S.CosmosAsync.get(url);
      for (const it of r?.items || []) {
        const t = it?.track;
        if (t?.uri) tracks.push(track(t.uri, t.name, artistNames(t.artists).join(", "), t.album?.name, t.duration_ms, bestImage(t.album?.images), tracks.length + 1));
      }
      url = r?.next;
    }
    return tracks;
  }

  /** Альбом: Web API клиента (с обложкой и всеми страницами). */
  async function album(uri) {
    const a = await S.CosmosAsync.get("https://api.spotify.com/v1/albums/" + idOf(uri));
    const cover = bestImage(a?.images);
    const items = [...(a?.tracks?.items || [])];
    let url = a?.tracks?.next;
    while (url) {
      const r = await S.CosmosAsync.get(url);
      items.push(...(r?.items || []));
      url = r?.next;
    }
    return { name: a?.name || "Альбом", tracks: items.map((t, k) => track(t.uri, t.name, artistNames(t.artists).join(", "), a?.name, t.duration_ms, cover, k + 1)) };
  }

  // ---------- действия ----------

  async function download(uris, contextUri) {
    uris = (uris || []).filter(Boolean);
    const u = uris[0] || contextUri;
    try {
      if (uris.length > 1 && uris.every(isTrack)) {
        say(`Собираю ${uris.length} трек(ов)…`);
        return sendList("Spotify — выбранные треки", await tracksInfo(uris), "Выбранные треки");
      }
      if (isTrack(u)) {
        const t = await trackInfo(u).catch(() => null);
        // Один трек — списком из одного (с тегами и обложкой) или ссылкой, если сведения не получили.
        return t ? sendList(t.title, [t], `${t.artist} — ${t.title}`) : send({ url: "https://open.spotify.com/track/" + idOf(u) }, "Трек");
      }
      if (isPlaylist(u)) {
        say("Собираю плейлист…");
        const p = await playlist(u);
        return sendList(p.name, p.tracks, `«${p.name}»`);
      }
      if (isAlbum(u)) {
        say("Собираю альбом…");
        try {
          const a = await album(u);
          return sendList(a.name, a.tracks, `«${a.name}»`);
        } catch {
          return send({ url: "https://open.spotify.com/album/" + idOf(u) }, "Альбом");
        }
      }
      if (isArtist(u)) {
        // Дискография — программа соберёт её сама (альбомы, EP, синглы по годам).
        return send({ url: "https://open.spotify.com/artist/" + idOf(u) }, "Дискография");
      }
      if (isLiked(u)) {
        say("Собираю «Любимые треки»…");
        return sendList("Любимые треки", await liked(), "«Любимые треки»");
      }
      say("Это нельзя скачать", true);
    } catch (e) {
      say("Ошибка: " + (e?.message || e), true);
    }
  }

  // Меню правой кнопки: трек(и), плейлист, альбом, исполнитель, «Любимые треки».
  const can = (uris, _uids, ctx) => {
    const u = uris?.[0] || ctx;
    return !!u && (isTrack(u) || isPlaylist(u) || isAlbum(u) || isArtist(u) || isLiked(u));
  };
  new S.ContextMenu.Item(NAME, (uris, _uids, ctx) => download(uris, ctx), can, "download").register();

  // Кнопка в плеере — текущий трек.
  if (S.Playbar?.Button) {
    // Родной значок Spotify «download» — как у соседних кнопок; свой рисунок — только если его нет.
    const icon = S.SVGIcons?.download ? "download" : ARROW;
    new S.Playbar.Button(NAME, icon, () => {
      const uri = S.Player?.data?.item?.uri;
      uri ? download([uri]) : say("Сейчас ничего не играет", true);
    }, false, false);
  }

  // Кнопка в шапке плейлиста / альбома / «Любимых треков» / исполнителя — рядом с большой Play.
  const pageUri = () => {
    const p = S.Platform.History?.location?.pathname || "";
    let m;
    if ((m = /^\/(playlist|album|artist)\/([A-Za-z0-9]+)/.exec(p))) return `spotify:${m[1]}:${m[2]}`;
    if (/^\/collection\/tracks/.test(p)) return "spotify:collection:tracks";
    return null;
  };
  const style = document.createElement("style");
  style.textContent =
    ".vgd-head{display:inline-flex;align-items:center;justify-content:center;width:40px;height:40px;margin:0 4px;" +
    "border:0;border-radius:50%;background:transparent;color:var(--text-subdued,#b3b3b3);cursor:pointer}" +
    ".vgd-head:hover{color:var(--text-base,#fff);transform:scale(1.06)}.vgd-head svg{width:24px;height:24px}";
  document.head.appendChild(style);
  function addHeaderButton() {
    const uri = pageUri();
    if (!uri) return;
    const bar = document.querySelector('[data-testid="action-bar-row"], .main-actionBar-ActionBarRow');
    if (!bar || bar.querySelector(".vgd-head")) return;
    const b = document.createElement("button");
    b.className = "vgd-head";
    b.title = b.ariaLabel = NAME;
    b.innerHTML = ARROW;
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      const now = pageUri();
      if (now) download([now]);
    });
    const play = bar.querySelector('[data-testid="play-button"], .main-playButton-PlayButton');
    const slot = play && [...bar.children].find((c) => c.contains(play));
    slot ? slot.after(b) : bar.prepend(b);
  }
  new MutationObserver(addHeaderButton).observe(document.body, { childList: true, subtree: true });
  addHeaderButton();
})();
