package io.github.vivegass.vgd

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.vivegass.vgd.ui.App
import io.github.vivegass.vgd.ui.Palette
import io.github.vivegass.vgd.ui.Style
import io.github.vivegass.vgd.ui.VgdTheme

class MainActivity : ComponentActivity() {
    override fun onResume() {
        super.onResume()
        // вернулись из «Установки неизвестных приложений» — поставить обновление
        Updater.resumeInstall(this)
    }

    private var style by mutableStateOf(Style.AUTO)
    private var colors by mutableStateOf(Palette.AUTO)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        style = Style.pref(this)
        colors = Style.colors(this)
        setContent {
            VgdTheme(style, colors) {
                App(
                    style = style,
                    colors = colors,
                    onStyle = { Style.setPref(this, it); style = it },
                    onColors = { Style.setColors(this, it); colors = it },
                )
            }
        }
    }
}
