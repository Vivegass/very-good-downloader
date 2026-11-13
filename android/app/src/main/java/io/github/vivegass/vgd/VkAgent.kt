package io.github.vivegass.vgd

import android.annotation.SuppressLint
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import io.github.vivegass.vgd.core.VgdException
import io.github.vivegass.vgd.core.VkSearcher
import io.github.vivegass.vgd.core.VkTrack
import io.github.vivegass.vgd.core.vkAgentJs
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/**
 * Поиск музыки в VK от аккаунта пользователя — «невидимый браузер», как WebView2 на ПК:
 * скрытый WebView с vk.com (сессия — из окна входа `LoginActivity`, общий CookieManager) и тот же
 * `vk_agent.js` из ядра. Ядро зовёт `search` из своей нити; WebView живёт в главной.
 */
object VkAgent : VkSearcher {
    /** Тот же «настольный» браузер, что и в окне входа: мобильная m.vk.com устроена иначе. */
    const val DESKTOP_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36"

    private val main = Handler(Looper.getMainLooper())
    private lateinit var app: Context
    private var web: WebView? = null
    @Volatile private var state: JSONObject? = null
    private var stateLatch = CountDownLatch(1)
    private val waiting = ConcurrentHashMap<Long, Pair<CountDownLatch, Array<JSONObject?>>>()
    private val nextRid = AtomicLong(1)

    fun attach(context: Context) {
        app = context.applicationContext
    }

    private fun prefs() = app.getSharedPreferences("vk", Context.MODE_PRIVATE)

    /** Вход по последней проверке (пишут окно входа и агент). */
    override fun loggedIn(): Boolean = prefs().getBoolean("logged", false)

    fun setLogged(v: Boolean) = prefs().edit().putBoolean("logged", v).apply()

    /** Обёртка для страницы: `window.ipc.postMessage` (как в WebView2) → Kotlin. */
    fun script(): String = "window.ipc = { postMessage: function (s) { VgdBridge.post(s); } };\n" + vkAgentJs()

    private class Bridge {
        @JavascriptInterface
        fun post(s: String) {
            val o = try { JSONObject(s) } catch (_: Exception) { return }
            if (o.optString("type") == "state") {
                state = o
                if (o.has("logged")) setLogged(o.optBoolean("logged"))
                stateLatch.countDown()
                return
            }
            val rid = o.optLong("rid", -1)
            waiting[rid]?.let { (latch, box) ->
                box[0] = o
                latch.countDown()
            }
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun ensureWeb() {
        if (web != null) return
        stateLatch = CountDownLatch(1)
        state = null
        val w = WebView(app)
        w.settings.javaScriptEnabled = true
        w.settings.domStorageEnabled = true
        w.settings.userAgentString = DESKTOP_UA
        CookieManager.getInstance().setAcceptCookie(true)
        w.addJavascriptInterface(Bridge(), "VgdBridge")
        w.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView, url: String) {
                view.evaluateJavascript(script(), null)
            }
        }
        w.loadUrl("https://vk.com/audio")
        web = w
    }

    /** Закрыть агента (после выхода из VK или смены аккаунта). */
    fun reset() = main.post {
        web?.destroy()
        web = null
    }

    override fun search(q: String, count: UInt): List<VkTrack> =
        call("window.__vgdSearch && window.__vgdSearch(%d, ${JSONObject.quote(q)}, ${count.toLong()})")

    override fun byId(ids: String): List<VkTrack> =
        call("window.__vgdById && window.__vgdById(%d, ${JSONObject.quote(ids)})")

    /** Защёлка «страница загрузилась» — та, что создал `ensureWeb` (в главной нити), а не прежняя. */
    private fun prepare(): CountDownLatch {
        val done = CountDownLatch(1)
        var latch: CountDownLatch? = null
        main.post {
            ensureWeb()
            latch = stateLatch
            done.countDown()
        }
        done.await(10, TimeUnit.SECONDS)
        return latch ?: stateLatch
    }

    /** Выполнить команду `vk_agent.js` (`%d` — номер запроса) и дождаться треков. */
    private fun call(jsTemplate: String): List<VkTrack> {
        if (!loggedIn()) throw VgdException.Failed("нет входа в VK — «Настройки → Аккаунты → войти в VK»")
        // ждём, пока страница VK загрузится и скажет, есть ли вход
        if (!prepare().await(40, TimeUnit.SECONDS)) {
            reset()
            throw VgdException.Failed("страница VK не загрузилась (нет связи)")
        }
        val st = state
        if (st == null || !st.optBoolean("logged") || !st.optBoolean("api")) {
            reset()
            throw VgdException.Failed("нет входа в VK — «Настройки → Аккаунты → войти в VK»")
        }
        val rid = nextRid.getAndIncrement()
        val latch = CountDownLatch(1)
        val box = arrayOfNulls<JSONObject>(1)
        waiting[rid] = latch to box
        val js = jsTemplate.replace("%d", rid.toString())
        main.post { web?.evaluateJavascript(js, null) }
        try {
            if (!latch.await(30, TimeUnit.SECONDS)) throw VgdException.Failed("VK не ответил за 30 с")
        } finally {
            waiting.remove(rid)
        }
        val r = box[0]!!
        r.optString("error").takeIf { it.isNotEmpty() }?.let { throw VgdException.Failed("VK: $it") }
        val items = r.optJSONArray("items") ?: return emptyList()
        return (0 until items.length()).mapNotNull { i ->
            val t = items.optJSONObject(i) ?: return@mapNotNull null
            val url = t.optString("url").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            VkTrack(
                url = url,
                title = t.optString("title"),
                artist = t.optString("artist"),
                duration = if (t.has("duration") && !t.isNull("duration")) t.optDouble("duration") else null,
                cover = t.optString("cover").takeIf { it.isNotEmpty() },
            )
        }
    }
}
