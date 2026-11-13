// Невидимый браузер VK для программы Very Good Downloader (окно WebView2, вход пользователя).
// Ищет треки через API VK от имени вошедшего пользователя и отдаёт программе ссылки на звук.
// Расшифровка ссылок — та же, что в extension/vk-main.js.
(() => {
  if (window.__vgdVk) return;
  window.__vgdVk = 1;
  const post = (o) => window.ipc.postMessage(JSON.stringify(o));

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

  const bigCover = (album) => {
    const t = album && album.thumb;
    return (t && (t.photo_1200 || t.photo_600 || t.photo_300)) || undefined;
  };

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

  /** Поиск: audio.search от имени пользователя; в ответе — только треки со ссылкой на звук. */
  window.__vgdSearch = async (rid, q, count) => {
    try {
      const api = window.vkApi && typeof window.vkApi.api === "function" ? window.vkApi : null;
      if (!api) return post({ rid, error: "нет входа в VK" });
      const res = await api.api("audio.search", { q, count: count || 10, auto_complete: 1, sort: 2, v: "5.204" });
      const items = (Array.isArray(res) ? res : res?.items || []).map(fromApi).filter((t) => t.url);
      post({ rid, items });
    } catch (e) {
      post({ rid, error: String((e && (e.error_msg || e.message)) || JSON.stringify(e)) });
    }
  };

  const state = () =>
    post({ type: "state", logged: !!(window.vk && +window.vk.id), api: !!(window.vkApi && window.vkApi.api), url: location.href });
  const later = () => setTimeout(state, 1500);
  document.readyState === "complete" ? later() : addEventListener("load", later);
})();
