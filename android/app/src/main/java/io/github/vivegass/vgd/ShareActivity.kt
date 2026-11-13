package io.github.vivegass.vgd

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import io.github.vivegass.vgd.core.extractUrl

/**
 * «Поделиться» → Very Good Downloader: прозрачный экран берёт ссылку, ставит её в очередь
 * фоновой службы и сразу закрывается (как фоновая загрузка на ПК). Об окончании скажет уведомление.
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val url = linkFrom(intent)
        if (url == null) {
            Toast.makeText(this, R.string.share_no_link, Toast.LENGTH_SHORT).show()
        } else {
            Vgd.add(this, url)
            Toast.makeText(this, R.string.share_added, Toast.LENGTH_SHORT).show()
        }
        finish()
    }

    companion object {
        /** Ссылка из «Поделиться» (текст), «Открыть с помощью» (адрес) или выделенного текста. */
        fun linkFrom(intent: Intent?): String? {
            intent ?: return null
            val text = when (intent.action) {
                Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)
                Intent.ACTION_PROCESS_TEXT -> intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()
                Intent.ACTION_VIEW -> intent.dataString
                else -> null
            } ?: return null
            return extractUrl(text)
        }
    }
}
