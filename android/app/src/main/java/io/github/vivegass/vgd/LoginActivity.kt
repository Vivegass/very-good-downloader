@file:OptIn(ExperimentalMaterial3Api::class)

package io.github.vivegass.vgd

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import io.github.vivegass.vgd.ui.Style
import io.github.vivegass.vgd.ui.VgdTheme
import java.io.File

/**
 * Вход на сайт в окне программы: пароль вводится на странице самого сайта, программа его не видит.
 * - VK: сессия остаётся в WebView (общий CookieManager) — ею пользуется скрытый агент поиска (`VkAgent`);
 * - YouTube: после входа куки сохраняются в `cookies.txt` (для 18+ и закрытых видео; правило «куки только
 *   при повторе» остаётся). Google может не пустить во встроенный браузер — тогда `cookies.txt` с ПК.
 */
class LoginActivity : ComponentActivity() {
    private var web: WebView? = null

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val site = intent.getStringExtra(EXTRA_SITE) ?: VK
        val title = if (site == VK) "Вход в VK" else "Вход в YouTube"
        val start = if (site == VK) "https://vk.com/" else
            "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fwww.youtube.com%2F"
        setContent {
            VgdTheme(Style.pref(this), Style.colors(this)) {
                Scaffold(
                    topBar = {
                        TopAppBar(
                            title = { Text(title) },
                            actions = { TextButton(onClick = { done(site) }) { Text("готово") } },
                        )
                    },
                ) { pad ->
                    AndroidView(
                        modifier = Modifier.fillMaxSize().padding(pad),
                        factory = { ctx ->
                            WebView(ctx).apply {
                                settings.javaScriptEnabled = true
                                settings.domStorageEnabled = true
                                if (site == VK) {
                                    settings.userAgentString = VkAgent.DESKTOP_UA
                                    addJavascriptInterface(StateBridge(), "VgdBridge")
                                }
                                CookieManager.getInstance().setAcceptCookie(true)
                                CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)
                                webViewClient = object : WebViewClient() {
                                    override fun onPageFinished(view: WebView, url: String) {
                                        if (site == VK) view.evaluateJavascript(VkAgent.script(), null)
                                    }
                                }
                                loadUrl(start)
                                web = this
                            }
                        },
                    )
                }
            }
        }
    }

    /** Сообщения vk_agent.js в окне входа: только «вошёл / не вошёл». */
    private class StateBridge {
        @android.webkit.JavascriptInterface
        fun post(s: String) {
            val o = try { org.json.JSONObject(s) } catch (_: Exception) { return }
            if (o.optString("type") == "state" && o.has("logged")) VkAgent.setLogged(o.optBoolean("logged"))
        }
    }

    private fun done(site: String) {
        CookieManager.getInstance().flush()
        if (site == VK) {
            VkAgent.reset() // агент перезагрузит страницу уже с новой сессией
        } else {
            val n = saveYoutubeCookies(this)
            Vgd.addLog(if (n > 0) "YouTube: сохранено кук — $n (cookies.txt)" else "YouTube: вход не найден — куки не сохранены")
        }
        setResult(RESULT_OK)
        finish()
    }

    override fun onDestroy() {
        web?.destroy()
        super.onDestroy()
    }

    companion object {
        const val EXTRA_SITE = "site"
        const val VK = "vk"
        const val YOUTUBE = "youtube"

        fun intent(ctx: Context, site: String) = Intent(ctx, LoginActivity::class.java).putExtra(EXTRA_SITE, site)

        fun cookiesFile(ctx: Context) = File(File(ctx.filesDir, "vgd"), "cookies.txt")

        /**
         * Куки youtube.com / google.com из WebView → `cookies.txt` (формат Netscape, его читает yt-dlp).
         * WebView отдаёт только «имя=значение», поэтому домен — общий, срок — на год вперёд.
         */
        fun saveYoutubeCookies(ctx: Context): Int {
            val cm = CookieManager.getInstance()
            val expires = System.currentTimeMillis() / 1000 + 365L * 24 * 3600
            val lines = mutableListOf("# Netscape HTTP Cookie File", "# Very Good Downloader: вход в YouTube на телефоне")
            for ((url, domain) in listOf("https://www.youtube.com" to ".youtube.com", "https://accounts.google.com" to ".google.com")) {
                val raw = cm.getCookie(url) ?: continue
                raw.split(";").map { it.trim() }.filter { it.contains('=') }.forEach { kv ->
                    val (k, v) = kv.split('=', limit = 2)
                    lines += listOf(domain, "TRUE", "/", "TRUE", expires.toString(), k, v).joinToString("\t")
                }
            }
            val n = lines.size - 2
            if (n <= 0) return 0
            val f = cookiesFile(ctx)
            f.parentFile?.mkdirs()
            f.writeText(lines.joinToString("\n") + "\n")
            return n
        }
    }
}
