// Запуск программы, когда её окно закрыто: открываем vgd://add?url=… из страницы расширения
// (из фоновой части браузер внешние протоколы не открывает), ждём, пока программа ответит, и закрываемся.

const url = new URLSearchParams(location.search).get("u") || "";
const launch = url ? "vgd://add?url=" + encodeURIComponent(url) : "vgd://add";

async function appIsUp() {
  try {
    const r = await fetch("http://127.0.0.1:38517/ping", { headers: { "X-VGD": "1" } });
    return r.ok;
  } catch {
    return false;
  }
}

// Ссылкой с кликом — так браузер надёжнее показывает запрос на открытие приложения.
const a = document.createElement("a");
a.href = launch;
document.body.appendChild(a);
a.click();

let tries = 0;
const timer = setInterval(async () => {
  tries += 1;
  if (await appIsUp()) {
    clearInterval(timer);
    // Если ждал список треков (VK) — background отправит его сейчас.
    await new Promise((r) => chrome.runtime.sendMessage({ type: "vgd-app-up" }, () => r()));
    document.getElementById("msg").textContent = "готово — ссылка в программе ✓";
    document.getElementById("hint").textContent = "";
    setTimeout(() => window.close(), 800);
  } else if (tries === 40) {
    clearInterval(timer);
    document.getElementById("msg").textContent = "программа не запустилась";
    const hint = document.getElementById("hint");
    hint.textContent = "откройте very good downloader вручную, в «настройки» → «расширение для браузера» нажмите «включить запуск» и попробуйте ещё раз. ";
    const again = document.createElement("a");
    again.href = launch;
    again.textContent = "повторить";
    hint.appendChild(again);
  }
}, 500);
