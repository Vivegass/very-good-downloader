// Вход в аккаунт для программы (YouTube / YouTube Music, SoundCloud).
// Если в программе включено «брать звук от аккаунта», расширение передаёт ей куки этого сайта —
// только в программу на этом компьютере (127.0.0.1), больше никуда. С ними YouTube Music Premium
// отдаёт AAC 256 kbps, а SoundCloud Go+ — полные треки в 256 kbps.
// Программа открывает сайт с меткой #vgd-login — тогда передаём сразу и говорим, получилось ли.
// Без метки — тихо, не чаще раза в полчаса (куки входа время от времени меняются).

(() => {
  const service = /(^|\.)soundcloud\.com$/.test(location.hostname) ? "soundcloud" : "youtube";
  const force = location.hash.includes("vgd-login");

  function say(text, ok) {
    const show = () => {
      const el = document.createElement("div");
      el.textContent = text;
      el.style.cssText =
        "position:fixed;right:24px;bottom:24px;z-index:2147483647;max-width:360px;padding:12px 16px;border-radius:12px;" +
        "font:14px/1.4 system-ui,sans-serif;color:#fff;box-shadow:0 6px 24px rgba(0,0,0,.3);background:" +
        (ok ? "#2e7d32" : "#b3261e");
      document.documentElement.appendChild(el);
      setTimeout(() => el.remove(), 7000);
    };
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", show);
    else show();
  }

  try {
    chrome.runtime.sendMessage({ type: "vgd-cookies", service, force }, (res) => {
      if (chrome.runtime.lastError || !force || !res) return;
      const site = service === "soundcloud" ? "SoundCloud" : "YouTube";
      if (res.state === "saved" && res.logged) say(`Вход в ${site} передан в Very Good Downloader ✓`, true);
      else if (res.state === "saved") say(`Вы не вошли в ${site} в этом браузере — войдите и обновите страницу.`, false);
      else if (res.state === "unwanted") say(`В программе вход в ${site} выключен — включите в «Настройки → аккаунты».`, false);
      else if (res.state === "offline") say("Very Good Downloader не запущен — откройте программу и нажмите «войти через браузер» ещё раз.", false);
      else say("Не удалось передать вход: " + (res.error || "ошибка"), false);
    });
  } catch {
    // расширение перезагрузили — вкладка о нём не знает
  }
})();
