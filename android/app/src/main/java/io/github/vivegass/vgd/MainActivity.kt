package io.github.vivegass.vgd

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.vivegass.vgd.ui.App
import io.github.vivegass.vgd.ui.Style
import io.github.vivegass.vgd.ui.VgdTheme

class MainActivity : ComponentActivity() {
    private var style by mutableStateOf(Style.AUTO)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        style = Style.pref(this)
        setContent {
            VgdTheme(style) {
                App(style = style, onStyle = { Style.setPref(this, it); style = it })
            }
        }
    }
}
