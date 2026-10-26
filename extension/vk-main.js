// Выполняется в контексте самой страницы VK (world: MAIN).
// 1) Новая разметка VK (React) не пишет название трека в атрибуты, поэтому берём его из свойств
//    компонента строки и кладём в data-vgd-audio — оттуда его читает vk.js.
// 2) По запросу vk.js получаем ссылки на звук треков (как VK Music Saver): API VK от имени
//    вошедшего пользователя (audio.getById), а для старой разметки — al_audio.php reload_audio.
//    Ссылки (HLS .m3u8) уходят в программу, она качает их своим yt-dlp/ffmpeg.

(() => {
  const ROWS = '[class*="AudioRow__root"], [data-testentitytag="audio"]';

  function fiberOf(el) {
    const k = Object.keys(el).find((x) => x.startsWith("__reactFiber$") || x.startsWith("__reactInternalInstance$"));
    return k ? el[k] : null;
  }

  /** Объект трека из API VK, который новая разметка держит в свойствах строки (как у VK Music Saver). */
  function apiAudioOf(p) {
    if (!p || typeof p !== "object") return null;
    return (
      p.track?.entity?.apiAudio ||
      p.episode?.entity?.apiAudio ||
      p.audio?.entity?.apiAudio ||
      p.entity?.apiAudio ||
      p.apiAudio ||
      (p.audio && p.audio.id != null && p.audio.url != null ? p.audio : null)
    );
  }

  function fromApi(a) {
    const owner = a.owner_id ?? a.ownerId;
    const artist = a.artist || a.performer || (Array.isArray(a.main_artists) && a.main_artists.map((x) => x.name).join(", ")) || "";
    return {
      title: (a.title || "") + (a.subtitle ? ` (${a.subtitle})` : ""),
      artist,
      duration: +a.duration || null,
      id: owner != null && a.id != null ? `${owner}_${a.id}` : undefined,
      accessKey: a.access_key || a.accessKey || undefined,
      url: unmask(a.url) || undefined,
      cover: bigCover(a.album),
      album: a.album?.title || undefined,
    };
  }

  /** Ищем в свойствах компонентов объект трека: есть title и исполнитель. */
  function findAudio(obj, depth) {
    const api = apiAudioOf(obj);
    if (api && api.title) return fromApi(api);
    if (!obj || typeof obj !== "object" || depth > 3) return null;
    const artist = obj.performer || obj.artist || (Array.isArray(obj.mainArtists) && obj.mainArtists.map((a) => a.name).join(", "));
    if (typeof obj.title === "string" && typeof artist === "string" && obj.title) {
      const owner = obj.ownerId ?? obj.owner_id;
      const hashes = typeof obj.hashes === "string" ? obj.hashes.split("/") : [];
      return {
        title: obj.title + (obj.subtitle ? ` (${obj.subtitle})` : ""),
        artist,
        duration: +obj.duration || null,
        id: owner != null && obj.id != null ? `${owner}_${obj.id}` : obj.fullId || obj.id || undefined,
        // для получения ссылки на звук
        accessKey: obj.accessKey || obj.access_key || undefined,
        actionHash: obj.actionHash || hashes[2] || undefined,
        urlHash: obj.urlHash || hashes[5] || undefined,
      };
    }
    for (const key of ["audio", "track", "item", "data", "entity", "apiAudio"]) {
      const r = findAudio(obj[key], depth + 1);
      if (r) return r;
    }
    return null;
  }

  function scan() {
    document.querySelectorAll(ROWS).forEach((row) => {
      if (row.dataset.vgdAudio && row.dataset.vgdAudio.includes('"id"')) return;
      let f = fiberOf(row);
      for (let i = 0; f && i < 25; i++, f = f.return) {
        const a = findAudio(f.memoizedProps, 0);
        if (a) {
          row.dataset.vgdAudio = JSON.stringify(a);
          return;
        }
      }
    });
  }

  setInterval(() => {
    try {
      scan();
    } catch (e) {
      /* разметка VK поменялась — vk.js возьмёт название из текста строки */
    }
  }, 1000);

  // ---------- ссылки на звук ----------

  /** VK прячет часть ссылок за «audio_api_unavailable»: настоящая ссылка закодирована в ?extra=.
   *  Это обратное преобразование того, что делает плеер VK (своя base64-азбука и цепочка операций). */
  const ABC = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN0PQRSTUVWXYZO123456789+/=";

  function b64(s) {
    if (!s || s.length % 4 === 1) return false;
    let out = "";
    let acc = 0;
    for (let i = 0, n = 0; i < s.length; i++) {
      const v = ABC.indexOf(s[i]);
      if (v < 0) continue;
      acc = n % 4 ? acc * 64 + v : v;
      if (n++ % 4) out += String.fromCharCode(255 & (acc >> ((-2 * n) & 6)));
    }
    return out;
  }

  function shuffleOrder(len, seed) {
    const order = [];
    seed = Math.abs(seed);
    for (let i = len; i--; ) {
      seed = ((len * (i + 1)) ^ (seed + i)) % len;
      order[i] = seed;
    }
    return order;
  }

  const OPS = {
    v: (s) => s.split("").reverse().join(""),
    r: (s, shift) => {
      const abc2 = ABC + ABC;
      return s
        .split("")
        .map((c) => {
          const i = abc2.indexOf(c);
          return i < 0 ? c : abc2.substr(i - shift, 1);
        })
        .join("");
    },
    s: (s, seed) => {
      const len = s.length;
      if (!len) return s;
      const order = shuffleOrder(len, seed);
      const a = s.split("");
      for (let i = 1; i < len; i++) a[i] = a.splice(order[len - 1 - i], 1, a[i])[0];
      return a.join("");
    },
    i: (s, seed) => OPS.s(s, seed ^ ((window.vk && +window.vk.id) || 0)),
    x: (s, key) => {
      const k = String(key).charCodeAt(0);
      return s
        .split("")
        .map((c) => String.fromCharCode(c.charCodeAt(0) ^ k))
        .join("");
    },
  };

  function unmask(url) {
    if (!url || url.indexOf("audio_api_unavailable") < 0) return url || "";
    try {
      const parts = url.split("?extra=")[1].split("#");
      let res = b64(parts[0]);
      const chain = parts[1] ? b64(parts[1]) : "";
      if (!res || typeof chain !== "string") return "";
      const steps = chain ? chain.split("\t") : [];
      for (let i = steps.length; i--; ) {
        const args = steps[i].split("\v");
        const op = OPS[args.shift()];
        if (!op) return "";
        res = op(res, ...args.map((a) => (/^-?\d+$/.test(a) ? +a : a)));
      }
      return /^https?:/.test(res) ? res : "";
    } catch {
      return "";
    }
  }

  const chunks = (arr, n) => Array.from({ length: Math.ceil(arr.length / n) }, (_, i) => arr.slice(i * n, i * n + n));
  const bigCover = (album) => {
    const t = album && album.thumb;
    return (t && (t.photo_1200 || t.photo_600 || t.photo_300)) || undefined;
  };

  /** API VK от имени вошедшего пользователя (тот же объект, которым пользуется сама страница). */
  async function viaApi(items, out) {
    const api = window.vkApi && typeof window.vkApi.api === "function" ? window.vkApi : null;
    if (!api) return;
    for (const part of chunks(items, 100)) {
      try {
        const ids = part.map((t) => (t.accessKey ? `${t.id}_${t.accessKey}` : t.id)).join(",");
        const res = await api.api("audio.getById", { audios: ids, v: "5.204" });
        for (const a of Array.isArray(res) ? res : res?.items || []) {
          const url = unmask(a && a.url);
          if (url) out[`${a.owner_id}_${a.id}`] = { url, cover: bigCover(a.album), album: a.album?.title || undefined };
        }
      } catch (e) {
        console.warn("[Very Good Downloader] audio.getById:", e);
      }
    }
  }

  /** Старый способ сайта: al_audio.php reload_audio, нужны хеши трека (по 10 за запрос). */
  async function viaReload(items, out) {
    const post = window.ajax && typeof window.ajax.promisifiedPost === "function" ? window.ajax.promisifiedPost.bind(window.ajax) : null;
    if (!post) return;
    const todo = items.filter((t) => !out[t.id] && t.actionHash && t.urlHash);
    for (const part of chunks(todo, 10)) {
      try {
        const ids = part.map((t) => `${t.id}_${t.actionHash}_${t.urlHash}`).join(",");
        const [list] = await post("al_audio.php", { act: "reload_audio", ids, al: 1 });
        for (const a of Array.isArray(list) ? list : []) {
          // кортеж VK: [id, owner, url, title, performer, duration, …, 14: обложки «маленькая,большая»]
          if (!Array.isArray(a)) continue;
          const url = unmask(a[2]);
          const covers = String(a[14] || "").split(",").filter((c) => c.startsWith("http"));
          if (url) out[`${a[1]}_${a[0]}`] = { url, cover: covers[covers.length - 1] };
        }
      } catch (e) {
        console.warn("[Very Good Downloader] reload_audio:", e);
      }
      await new Promise((r) => setTimeout(r, 300)); // сайт не любит частые запросы
    }
  }

  async function resolve(items) {
    const out = {};
    items = items.filter((t) => t && typeof t.id === "string" && /^-?\d+_\d+$/.test(t.id));
    await viaApi(items, out);
    await viaReload(items, out);
    return out;
  }

  // ---------- плейлист целиком через API (как VK Music Saver), без прокрутки строк ----------

  const PL_RE = /(?:audio_playlist|\/music\/(?:playlist|album)\/)(-?\d+)_(\d+)(?:(?:\/|%2F|_)([0-9a-f]{6,}))?/i;

  /** Ищем в свойствах компонентов объект плейлиста: id, владелец, название, но не трек. */
  function findPlaylist(obj, depth) {
    if (!obj || typeof obj !== "object" || depth > 3) return null;
    const owner = obj.owner_id ?? obj.ownerId;
    if (obj.id != null && owner != null && typeof obj.title === "string" && !("duration" in obj && "url" in obj)) {
      if ("count" in obj || "plays" in obj || "access_key" in obj || "accessKey" in obj || "followed" in obj || "is_following" in obj) {
        return { owner: String(owner), id: String(obj.id), key: obj.access_key || obj.accessKey || "", title: obj.title };
      }
    }
    for (const key of ["playlist", "audioPlaylist", "originalAttachment", "audio_playlist", "entity", "data", "item"]) {
      const r = findPlaylist(obj[key], depth + 1);
      if (r) return r;
    }
    return null;
  }

  /** Какой плейлист у этой шапки: из свойств React, из ссылок рядом, из адреса страницы. */
  function playlistOf(head, diag) {
    let f = head && fiberOf(head);
    for (let i = 0; f && i < 40; i++, f = f.return) {
      const p = findPlaylist(f.memoizedProps, 0);
      if (p) return (diag.plFrom = "react"), p;
    }
    const root = head && head.closest('[class*="AudioListHeader"], [class*="AudioListBoxHeader"], [class*="AudioListModalHeader"], [data-testid*="Header"], [role="dialog"]');
    for (const a of root ? root.querySelectorAll("a[href]") : []) {
      const m = PL_RE.exec(decodeURIComponent(a.getAttribute("href")));
      if (m) return (diag.plFrom = "link"), { owner: m[1], id: m[2], key: m[3] || "" };
    }
    const m = PL_RE.exec(decodeURIComponent(location.href));
    if (m) return (diag.plFrom = "url"), { owner: m[1], id: m[2], key: m[3] || "" };
    return null;
  }

  async function playlistTracks(head, diag) {
    const api = window.vkApi && typeof window.vkApi.api === "function" ? window.vkApi : null;
    diag.vkApi = !!api;
    const pl = playlistOf(head, diag);
    diag.playlist = pl ? `${pl.owner}_${pl.id}${pl.key ? "_" + pl.key : ""}` : null;
    if (!api || !pl) return null;
    let items = null;
    try {
      const entity = pl.key ? `${pl.owner}_${pl.id}_${pl.key}` : `${pl.owner}_${pl.id}`;
      const r = await api.api("audio.getAudioIdsBySource", { source: "playlist", entity_id: entity });
      const ids = (r?.audios || []).map((a) => a.audio_id).filter(Boolean);
      diag.ids = ids.length;
      items = [];
      for (const part of chunks(ids, 100)) {
        const res = await api.api("audio.getById", { audios: part.join(","), v: "5.204" });
        items.push(...(Array.isArray(res) ? res : res?.items || []));
      }
    } catch (e) {
      diag.err1 = String(e && (e.message || e.error_msg || e)).slice(0, 200);
      items = null;
    }
    if (!items || !items.length) {
      try {
        const r = await api.api("audio.get", { owner_id: pl.owner, playlist_id: pl.id, access_key: pl.key || undefined, count: 10000 });
        items = r?.items || (Array.isArray(r) ? r : []);
        diag.via = "audio.get";
      } catch (e) {
        diag.err2 = String(e && (e.message || e.error_msg || e)).slice(0, 200);
      }
    }
    const tracks = (items || []).filter((a) => a && a.title).map(fromApi);
    diag.tracks = tracks.length;
    diag.withUrl = tracks.filter((t) => t.url).length;
    // Название — из API: в окне плейлиста заголовок страницы принадлежит тому, что под окном.
    let title = pl.title;
    if (!title && tracks.length) {
      try {
        const p = await api.api("audio.getPlaylistById", { owner_id: pl.owner, playlist_id: pl.id, access_key: pl.key || undefined });
        title = p?.title || "";
      } catch (e) {
        diag.errTitle = String(e && (e.message || e.error_msg || e)).slice(0, 200);
      }
    }
    diag.title = title || null;
    return tracks.length ? { title, tracks } : null;
  }

  // Запросы от vk.js (изолированный мир расширения) через postMessage этой же страницы.
  window.addEventListener("message", (e) => {
    if (e.source !== window || !e.data) return;
    const { vgd, req } = e.data;
    if (vgd === "vk-resolve") {
      const items = e.data.items;
      const diag = { vkApi: !!(window.vkApi && window.vkApi.api), reload: !!(window.ajax && window.ajax.promisifiedPost) };
      resolve(Array.isArray(items) ? items : [])
        .catch((err) => ((diag.err = String(err).slice(0, 200)), {}))
        .then((found) => window.postMessage({ vgd: "vk-resolved", req, found, diag }, location.origin));
    } else if (vgd === "vk-playlist") {
      const diag = {};
      const head = document.querySelector(`[data-vgd-req="${req}"]`);
      playlistTracks(head, diag)
        .catch((err) => ((diag.err = String(err).slice(0, 200)), null))
        .then((list) => window.postMessage({ vgd: "vk-playlist-done", req, list, diag }, location.origin));
    }
  });
})();
