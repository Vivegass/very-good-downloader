// Very Good Downloader: передаёт ссылку в программу.
// 1) Окно открыто — POST на 127.0.0.1:38517, загрузка начинается сразу.
// 2) Окно закрыто — открываем ссылку vgd://add?url=…, Windows запускает программу.

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

async function sendToApp(url) {
  return post({ url });
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

/** Отправить ссылку; если программа не отвечает — запустить её через vgd://. */
async function download(rawUrl, tabId) {
  const url = cleanUrl(rawUrl);
  if (!/^https?:\/\//i.test(url)) return { ok: false, error: "это не ссылка на страницу" };
  if (await sendToApp(url)) {
    flash(tabId, "✓", "#2e7d32");
    return { ok: true, via: "app" };
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
  if (await post(payload)) return { ok: true, via: "app" };
  // Список длинный для ссылки vgd:// — запускаем программу пустой и отправляем список, когда она ответит.
  await chrome.storage.session.set({ pending: payload });
  await chrome.tabs.create({ url: chrome.runtime.getURL("launch.html?u=") });
  return { ok: true, via: "launch" };
}

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  if (!msg) return;
  // Кнопка на странице YouTube (youtube.js).
  if (msg.type === "vgd-download") {
    download(msg.url, sender.tab && sender.tab.id).then(reply);
    return true; // ответ придёт асинхронно
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
