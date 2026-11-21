// Very Good Downloader: передаёт ссылку в программу.
// 1) Окно открыто — POST на 127.0.0.1:38517, загрузка начинается сразу.
// 2) Программа закрыта — открываем ссылку vgd://add?url=…, Windows запускает её в фоне (без окна).
// Программа отвечает номером заявки; по нему страница узнаёт «скачано» (GET /status?t=…).

const APP = "http://127.0.0.1:38517";

/** Чистая ссылка: для YouTube — без «микса», времени и меток. */
function cleanUrl(raw) {
  try {
    const u = new URL(raw);
    const host = u.hostname.replace(/^www\.|^m\./, "");
    if (host === "youtube.com" && u.pathname === "/watch" && u.searchParams.get("v")) {
      const list = u.searchParams.get("list");
      const out = new URL("https://www.youtube.com/watch");
      out.searchParams.set("v", u.searchParams.get("v"));
      // Обычный плейлист оставляем (программа решит по настройке), «микс» RD… — нет.
      if (list && !list.startsWith("RD")) out.searchParams.set("list", list);
      return out.toString();
    }
    if (host === "youtu.be") {
      return "https://www.youtube.com/watch?v=" + u.pathname.slice(1);
    }
    return u.toString();
  } catch {
    return raw;
  }
}

async function post(payload) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 3000);
  try {
    const r = await fetch(APP + "/add", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-VGD": "1" },
      body: JSON.stringify(payload),
      signal: ctrl.signal,
    });
    return r.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

/** Отправить; ответ программы ({ok, added, ticket}) или null. */
async function postJson(payload) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 3000);
  try {
    const r = await fetch(APP + "/add", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-VGD": "1" },
      body: JSON.stringify(payload),
      signal: ctrl.signal,
    });
    return r.ok ? await r.json().catch(() => ({ ok: true })) : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** Состояние заявки: queued / running / done / error / unknown; null — программа не отвечает. */
async function ticketStatus(ticket) {
  try {
    const r = await fetch(APP + "/status?t=" + encodeURIComponent(ticket), { headers: { "X-VGD": "1" } });
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

/** Отправить ссылку; если программа не отвечает — запустить её через vgd://. */
async function download(rawUrl, tabId) {
  const url = cleanUrl(rawUrl);
  if (!/^https?:\/\//i.test(url)) return { ok: false, error: "это не ссылка на страницу" };
  const res = await postJson({ url });
  if (res) {
    flash(tabId, "✓", "#2e7d32");
    return { ok: true, via: "app", ticket: res.ticket };
  }
  // Окно закрыто: открываем свою вкладку, она запускает программу через vgd:// и сама закрывается.
  await chrome.tabs.create({ url: chrome.runtime.getURL("launch.html?u=" + encodeURIComponent(url)) });
  flash(tabId, "…", "#555555");
  return { ok: true, via: "launch" };
}

function flash(tabId, text, color) {
  if (tabId === undefined) return;
  chrome.action.setBadgeBackgroundColor({ tabId, color });
  chrome.action.setBadgeText({ tabId, text });
  setTimeout(() => chrome.action.setBadgeText({ tabId, text: "" }), 2500);
}

// Значок расширения: скачать то, что открыто во вкладке (YouTube, Pinterest, TikTok, VK…).
chrome.action.onClicked.addListener((tab) => {
  if (tab.url) download(tab.url, tab.id);
});

// Меню правой кнопки: ссылка, видео на странице или сама страница.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "vgd-link",
      title: "Скачать ссылку в Very Good Downloader",
      contexts: ["link"],
    });
    chrome.contextMenus.create({
      id: "vgd-page",
      title: "Скачать видео со страницы в Very Good Downloader",
      contexts: ["page", "video", "image"],
    });
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  const url = info.menuItemId === "vgd-link" ? info.linkUrl : info.pageUrl || (tab && tab.url);
  if (url) download(url, tab && tab.id);
});

// Список треков (VK), который ждёт запуска программы (в storage.session — фон браузер может выгрузить).

async function sendTracks(msg) {
  const payload = { name: msg.name, service: msg.service || "VK", tracks: msg.tracks, diag: msg.diag };
  const res = await postJson(payload);
  if (res) return { ok: true, via: "app", ticket: res.ticket };
  // Список длинный для ссылки vgd:// — запускаем программу пустой и отправляем список, когда она ответит.
  await chrome.storage.session.set({ pending: payload });
  await chrome.tabs.create({ url: chrome.runtime.getURL("launch.html?u=") });
  return { ok: true, via: "launch" };
}

// Вход в YouTube / SoundCloud для программы (login.js): куки сайта → POST /cookies на 127.0.0.1.
// Программа берёт их, только если в ней включено «брать звук от аккаунта»; иначе отвечает unwanted.
const COOKIE_SITES = { youtube: ["youtube.com"], soundcloud: ["soundcloud.com"] };

/** Запрос к программе; ответ JSON или null (не запущена). */
async function askApp(method, path, body) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 4000);
  try {
    const r = await fetch(APP + path, {
      method,
      headers: { "Content-Type": "application/json", "X-VGD": "1" },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: ctrl.signal,
    });
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

async function syncCookies(service, force) {
  const domains = COOKIE_SITES[service];
  if (!domains || !chrome.cookies) return { state: "error", error: "браузер не дал доступ к кукам" };
  const key = "cookies-next-" + service;
  const now = Date.now();
  if (!force) {
    const next = (await chrome.storage.local.get(key))[key] || 0;
    if (now < next) return { state: "skipped" };
  }
  // Сначала спрашиваем, нужен ли программе вход: если нет — куки из браузера не уходят вовсе.
  const asked = await askApp("GET", "/cookies?service=" + service);
  if (!asked || asked.state !== "wanted") {
    const res = asked || { state: "offline" };
    await chrome.storage.local.set({ [key]: now + (res.state === "offline" ? 2 : 30) * 60 * 1000 });
    return res;
  }
  const cookies = [];
  for (const domain of domains) {
    for (const c of await chrome.cookies.getAll({ domain })) {
      cookies.push({
        domain: c.domain,
        path: c.path,
        secure: c.secure,
        hostOnly: c.hostOnly,
        expirationDate: c.expirationDate || 0,
        name: c.name,
        value: c.value,
      });
    }
  }
  const res = (await askApp("POST", "/cookies", { service, cookies })) || { state: "offline" };
  // Программа закрыта — попробуем при следующем открытии сайта через 2 минуты, иначе через полчаса.
  await chrome.storage.local.set({ [key]: now + (res.state === "offline" ? 2 : 30) * 60 * 1000 });
  return res;
}

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  if (!msg) return;
  if (msg.type === "vgd-cookies") {
    syncCookies(msg.service, !!msg.force).then(reply, (e) => reply({ state: "error", error: String(e) }));
    return true;
  }
  // Кнопка на странице YouTube (youtube.js).
  if (msg.type === "vgd-download") {
    download(msg.url, sender.tab && sender.tab.id).then(reply);
    return true; // ответ придёт асинхронно
  }
  // Страница спрашивает, скачалось ли (common.js → watch).
  if (msg.type === "vgd-status") {
    ticketStatus(msg.ticket).then((s) => reply(s || { state: "offline" }));
    return true;
  }
  // Сохранить файл (обложку) в «Загрузки» браузера.
  if (msg.type === "vgd-save-file") {
    chrome.downloads
      .download({ url: msg.url, filename: (msg.name || "cover.jpg").replace(/[\\/:*?"<>|]/g, "_") })
      .then(() => reply({ ok: true }))
      .catch(() => reply({ ok: false }));
    return true;
  }
  // Кнопка «скачать плейлист» в VK (vk.js).
  if (msg.type === "vgd-tracks") {
    sendTracks(msg).then(reply);
    return true;
  }
  // launch.js: программа запустилась — отдаём отложенный список.
  if (msg.type === "vgd-app-up") {
    chrome.storage.session.get("pending").then(async ({ pending }) => {
      if (!pending) return reply({ ok: true });
      await chrome.storage.session.remove("pending");
      reply({ ok: await post(pending) });
    });
    return true;
  }
});

// Сообщаем программе, что расширение установлено (она покажет это в настройках).
function hello() {
  fetch(APP + "/hello", {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-VGD": "1" },
    body: JSON.stringify({ version: chrome.runtime.getManifest().version }),
  }).catch(() => {});
}
chrome.runtime.onStartup.addListener(hello);
chrome.runtime.onInstalled.addListener(hello);
hello();
