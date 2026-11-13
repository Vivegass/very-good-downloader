// Общее для всех сайтов: значок, отправка в программу и короткое уведомление.
// Подключается в manifest.json перед скриптом сайта (у них общее пространство имён).

const VGD = (() => {
  const ARROW =
    // Размер 24 — запасной: если стили расширения пропали, значок не растянется на пол-экрана.
    '<svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false">' +
    '<path d="M12 4.5v10m0 0-4-4m4 4 4-4M5.5 19h13" fill="none" stroke="currentColor" stroke-width="2" ' +
    'stroke-linecap="round" stroke-linejoin="round"/></svg>';

  /** Кнопка-значок «скачать». extra — дополнительные классы под стиль сайта. */
  function button(extra, title, onClick) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "vgd-icon " + (extra || "");
    b.title = title || "Скачать в Very Good Downloader";
    b.setAttribute("aria-label", b.title);
    b.innerHTML = '<span class="vgd-glyph">' + ARROW + "</span>";
    b.addEventListener(
      "click",
      (e) => {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        onClick(b, e);
      },
      true
    );
    return b;
  }

  let toastEl = null;
  let toastTimer = 0;
  /** Уведомление в правом нижнем углу. */
  function toast(text, kind) {
    if (!toastEl) {
      toastEl = document.createElement("div");
      toastEl.className = "vgd-toast";
      document.documentElement.appendChild(toastEl);
    }
    toastEl.textContent = text;
    toastEl.dataset.kind = kind || "";
    toastEl.classList.add("vgd-show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toastEl.classList.remove("vgd-show"), kind === "wait" ? 15000 : 3500);
  }

  /** Следить за заявкой в программе и сказать, когда скачалось (пока вкладка открыта, до 3 часов). */
  function watch(ticket, what) {
    const end = Date.now() + 3 * 3600 * 1000;
    let misses = 0;
    const tick = () => {
      if (Date.now() > end) return;
      try {
        chrome.runtime.sendMessage({ type: "vgd-status", ticket }, (s) => {
          if (chrome.runtime.lastError || !s || s.state === "offline" || s.state === "unknown") {
            if (++misses < 5) setTimeout(tick, 3000);
            return;
          }
          misses = 0;
          if (s.state === "done") toast(what + " — скачано ✓" + (s.text ? " (" + s.text + ")" : ""), "ok");
          else if (s.state === "error") toast(what + " — не скачалось: " + (s.text || "ошибка"), "err");
          else setTimeout(tick, 3000);
        });
      } catch {
        // расширение перезагрузили — вкладка о нём больше не знает
      }
    };
    setTimeout(tick, 3000);
  }

  function result(res, what) {
    if (chrome.runtime.lastError || !res || !res.ok) {
      toast("не удалось отправить в программу", "err");
    } else if (res.via === "app") {
      toast(what + " — в очереди Very Good Downloader", "ok");
      if (res.ticket) watch(res.ticket, what);
    } else {
      toast("запускаю Very Good Downloader в фоне…", "wait");
    }
  }

  function busy(b, on) {
    if (b) b.classList.toggle("vgd-busy", !!on);
  }

  /** Отправить ссылку (трек, плейлист, видео). */
  function sendUrl(url, b, what) {
    busy(b, true);
    chrome.runtime.sendMessage({ type: "vgd-download", url }, (res) => {
      busy(b, false);
      result(res, what || "ссылка");
    });
  }

  /** Отправить список треков (Spotify и VK отдают его только на странице). */
  function sendTracks(name, service, tracks, b, diag) {
    busy(b, true);
    chrome.runtime.sendMessage({ type: "vgd-tracks", name, service, tracks, diag }, (res) => {
      busy(b, false);
      result(res, tracks.length === 1 ? "трек" : `${tracks.length} треков`);
    });
  }

  function toSeconds(s) {
    const m = /(\d+):(\d{2})(?::(\d{2}))?/.exec(s || "");
    if (!m) return null;
    return m[3] ? +m[1] * 3600 + +m[2] * 60 + +m[3] : +m[1] * 60 + +m[2];
  }

  /** Повторять fn раз в секунду: сайты — одностраничные приложения и перерисовывают разметку. */
  function every(fn) {
    let timer = 0;
    const run = () => {
      // Расширение обновили, перезагрузили или выключили, а вкладка осталась открытой: браузер снимает
      // с неё наши стили, и кнопки остаются «голыми» (огромный значок). Убираем их и останавливаемся —
      // после обновления страницы кнопки вернутся.
      if (!alive()) {
        clearInterval(timer);
        cleanup();
        return;
      }
      try {
        fn();
      } catch (e) {
        /* разметка сайта поменялась — просто ждём следующего раза */
      }
    };
    run();
    timer = setInterval(run, 1000);
  }

  function alive() {
    try {
      return !!chrome.runtime.id;
    } catch {
      return false;
    }
  }

  function cleanup() {
    document
      .querySelectorAll(".vgd-icon, .vgd-toast, .vgd-vk-menu, .vgd-vk-hb, .vgd-vk-native, .vgd-vk-head, .vgd-am-wrap")
      .forEach((el) => el.remove());
  }

  return { button, toast, sendUrl, sendTracks, toSeconds, every, alive };
})();
