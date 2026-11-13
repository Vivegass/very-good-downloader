package io.github.vivegass.vgd

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.core.content.pm.ShortcutManagerCompat
import io.github.vivegass.vgd.core.extractUrl

/**
 * «Поделиться» → «VGD: скачать» / «VGD: звук» (или ярлык Direct Share «Видео» / «Звук»):
 * прозрачный экран берёт ссылку, ставит её в очередь фоновой службы и сразу закрывается
 * (как фоновая загрузка на ПК). Об окончании скажет уведомление.
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val url = linkFrom(intent)
        // «VGD: звук» (вторая цель) или ярлык Direct Share «Звук» — только звук
        val shortcut = intent?.getStringExtra(ShortcutManagerCompat.EXTRA_SHORTCUT_ID)
        val audio = shortcut == Shortcuts.AUDIO || componentName.className.endsWith(".ShareAudio")
        if (url == null) {
            Toast.makeText(this, R.string.share_no_link, Toast.LENGTH_SHORT).show()
        } else {
            Vgd.add(this, url, audio)
            Shortcuts.used(this, if (audio) Shortcuts.AUDIO else Shortcuts.VIDEO)
            Toast.makeText(this, if (audio) R.string.share_added_audio else R.string.share_added, Toast.LENGTH_SHORT).show()
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
