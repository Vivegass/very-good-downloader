package io.github.vivegass.vgd

import android.annotation.SuppressLint
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import io.github.vivegass.vgd.core.SpotifyPlaylistPage
import io.github.vivegass.vgd.core.VgdException
import io.github.vivegass.vgd.core.spotifyAgentJs
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Весь плейлист Spotify без расширения браузера и без ключа: скрытый WebView открывает
 * open.spotify.com/playlist/<id> (вход не нужен), `spotify_agent.js` из ядра прокручивает список и
 * собирает треки — как кнопка расширения на ПК. Встраиваемый плеер и Web API отдают не больше 100 треков
 * (с февраля 2026 API вообще не отдаёт содержимое чужих плейлистов).
 */
object SpotifyAgent : SpotifyPlaylistPage {
    private val main = Handler(Looper.getMainLooper())
    private lateinit var app: Context
    private val lock = Any()

    fun attach(context: Context) {
        app = context.applicationContext
    }

    private class Bridge(val latch: CountDownLatch, val box: Array<String?>) {
        @JavascriptInterface
        fun post(s: String) {
            val o = try { JSONObject(s) } catch (_: Exception) { return }
            if (o.optString("type") != "spotify" || latch.count == 0L) return
            box[0] = s
            latch.countDown()
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    override fun playlist(id: String): String = synchronized(lock) {
        val latch = CountDownLatch(1)
        val box = arrayOfNulls<String>(1)
        var web: WebView? = null
        main.post {
            val w = WebView(app)
            w.settings.javaScriptEnabled = true
            w.settings.domStorageEnabled = true
            w.settings.userAgentString = VkAgent.DESKTOP_UA
            w.settings.useWideViewPort = true
            w.settings.loadWithOverviewMode = true
            // страница рисуется и считает прокрутку, хотя WebView не на экране
            w.settings.offscreenPreRaster = true
            w.addJavascriptInterface(Bridge(latch, box), "VgdSpotify")
            w.webViewClient = object : WebViewClient() {
                override fun onPageFinished(view: WebView, url: String) {
                    view.evaluateJavascript("window.ipc = { postMessage: function (s) { VgdSpotify.post(s); } };\n" + spotifyAgentJs(), null)
                }
            }
            // список виртуальный: нужен «экран», иначе строки не появляются
            w.measure(View.MeasureSpec.makeMeasureSpec(1280, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(2400, View.MeasureSpec.EXACTLY))
            w.layout(0, 0, 1280, 2400)
            w.loadUrl("https://open.spotify.com/playlist/$id")
            web = w
        }
        try {
            // большой плейлист (1000+ треков) прокручивается несколько минут
            if (!latch.await(6, TimeUnit.MINUTES)) throw VgdException.Failed("страница Spotify не ответила за 6 минут")
        } finally {
            main.post { web?.destroy() }
        }
        val r = box[0]!!
        JSONObject(r).optString("error").takeIf { it.isNotEmpty() }?.let { throw VgdException.Failed(it) }
        r
    }
}
