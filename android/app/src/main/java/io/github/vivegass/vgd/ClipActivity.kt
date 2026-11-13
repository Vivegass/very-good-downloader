package io.github.vivegass.vgd

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.widget.Toast
import io.github.vivegass.vgd.core.extractUrl

/** Прозрачный экран для плитки: буфер обмена можно прочитать, только когда окно в фокусе. */
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
            Vgd.add(this, url)
            Toast.makeText(this, R.string.share_added, Toast.LENGTH_SHORT).show()
        }
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }
}
