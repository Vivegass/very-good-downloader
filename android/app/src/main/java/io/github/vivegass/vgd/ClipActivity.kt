package io.github.vivegass.vgd

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import io.github.vivegass.vgd.core.extractUrl

/**
 * Прозрачный экран для плитки «Скачать из буфера» и ярлыков на значке: буфер обмена можно прочитать,
 * только когда окно в фокусе.
 */
class ClipActivity : Activity() {
    private var done = false

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val text = cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString().orEmpty()
        val url = extractUrl(text)
        if (url == null) {
            Toast.makeText(this, R.string.clip_no_link, Toast.LENGTH_SHORT).show()
        } else {
            val audio = intent.getBooleanExtra(EXTRA_AUDIO, false)
            Vgd.add(this, url, audio)
            Shortcuts.used(this, if (audio) Shortcuts.AUDIO else Shortcuts.VIDEO)
            Toast.makeText(this, if (audio) R.string.share_added_audio else R.string.share_added, Toast.LENGTH_SHORT).show()
        }
        finish()
    }

    companion object {
        const val EXTRA_AUDIO = "io.github.vivegass.vgd.AUDIO"
    }
}
