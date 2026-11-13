package io.github.vivegass.vgd

import android.app.Application
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState
import io.github.vivegass.vgd.ui.DownloadContent
import io.github.vivegass.vgd.ui.LocalThumbs
import io.github.vivegass.vgd.ui.MainNav
import io.github.vivegass.vgd.ui.Palette
import io.github.vivegass.vgd.ui.QueueActions
import io.github.vivegass.vgd.ui.SettingsContent
import io.github.vivegass.vgd.ui.SettingsEnv
import io.github.vivegass.vgd.ui.SettingsHandler
import io.github.vivegass.vgd.ui.Style
import io.github.vivegass.vgd.ui.ThumbLoader
import io.github.vivegass.vgd.ui.VgdTheme
import org.json.JSONObject
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Скриншоты «Скачать» (очередь из 5 карточек) и «Настроек» во всех стилях, светлом и тёмном — без телефона:
 * Robolectric + Roborazzi. `gradlew testDebugUnitTest --tests *ScreensTest* -Pvgd.screens=<папка>`.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], application = Application::class, qualifiers = "w412dp-h915dp-xxhdpi")
class ScreensTest(private val style: String, private val dark: Boolean) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}-{1}")
        fun params() = Style.ALL.flatMap { s -> listOf(arrayOf<Any>(s, false), arrayOf<Any>(s, true)) }
    }

    @get:Rule
    val rule = createComposeRule()

    private val out = File(System.getProperty("vgd.screens") ?: "build/screens").also { it.mkdirs() }
    private val name = "$style-${if (dark) "dark" else "light"}"

    /** «Кадры» вместо превью из сети: небо, солнце, холмы — у каждого ролика свои цвета. */
    private val thumbs = ThumbLoader { url ->
        val h = url.hashCode()
        val hues = listOf(
            Color(0xFF2E3A87) to Color(0xFFFF8A65), Color(0xFF00695C) to Color(0xFFFFD54F),
            Color(0xFF4A148C) to Color(0xFFF06292), Color(0xFF0D47A1) to Color(0xFF81D4FA),
        )
        val (a, b) = hues[Math.floorMod(h, hues.size)]
        val img = ImageBitmap(320, 180)
        val c = Canvas(img)
        val p = Paint()
        p.shader = Brush.verticalGradient(listOf(a, b), 0f, 180f).let { (it as androidx.compose.ui.graphics.ShaderBrush).createShader(Size(320f, 180f)) }
        c.drawRect(0f, 0f, 320f, 180f, p)
        c.drawCircle(Offset(230f, 70f), 30f, Paint().apply { color = Color(0xFFFFF3E0) })
        val hill = Path().apply {
            moveTo(0f, 180f); lineTo(0f, 120f); quadraticTo(80f, 80f, 160f, 125f); quadraticTo(240f, 165f, 320f, 110f); lineTo(320f, 180f); close()
        }
        c.drawPath(hill, Paint().apply { color = a.copy(alpha = 0.85f) })
        img
    }

    private fun job(
        id: Int, url: String, title: String, state: JobState, stage: String, percent: Float = 0f, item: String = "",
        speed: String = "", eta: String = "", message: String = "", error: String = "", audio: Boolean = false, thumb: String = "",
    ) = JobInfo(
        id.toULong(), url, title, stage, item, percent, speed, eta, message, emptyList(), state, error, "", audio, null, "{}", thumb,
    )

    private val jobs = listOf(
        job(1, "https://www.youtube.com/watch?v=aqz-KE-bpKQ", "Big Buck Bunny 60fps 4K — Official Blender Foundation Short Film", JobState.RUNNING, "video",
            42f, speed = "4.1 МБ/с", eta = "00:27", message = "Видео 2160p · 612 МБ"),
        job(2, "https://open.spotify.com/playlist/37i9dQZF1DX4WYpdgoIcn6", "Chill Hits", JobState.RUNNING, "audio",
            63f, item = "12/72", speed = "1.2 МБ/с", eta = "00:04", message = "Звук · 3.7 МБ", audio = true, thumb = "cover://chill"),
        job(3, "https://soundcloud.com/artist/sets/summer-2026", "Summer Set 2026", JobState.QUEUED, "queued", audio = true),
        job(4, "https://www.youtube.com/watch?v=jNQXAC9IVRw", "Me at the zoo", JobState.DONE, "done", 100f, message = "файлов: 1"),
        job(5, "https://vk.com/audio-2001949618_118949618", "Steve Lacy — Dark Red", JobState.ERROR, "error", audio = true,
            error = "VK не показывает трек без входа — нашёл на YouTube Music, но запись не совпала"),
    )

    private val settings = JSONObject(
        """{"out_dir":"/storage/emulated/0/Download/Very Good Downloader","max_height":1080,"max_fps":60,"sort":true,"playlist":true,
           "subs":false,"sub_langs":"ru,en","thumbnail":true,"sponsorblock":true,"split_chapters":false,"source_log":true,"edit":"h264",
           "edit_replace":false,"music_format":"best","music_numbers":true,"music_no_replace":false,"music_verify":true,"lyrics":true,
           "vk_search":true,"yandex_acc":false}""",
    )

    private fun shot(file: String, content: @Composable () -> Unit) {
        rule.mainClock.autoAdvance = false
        rule.setContent {
            CompositionLocalProvider(LocalThumbs provides thumbs) {
                VgdTheme(style, Palette.AUTO, dark = dark, content = content)
            }
        }
        rule.mainClock.advanceTimeBy(500)
        rule.onRoot().captureRoboImage(File(out, file).absolutePath)
    }

    private val none = object : QueueActions {
        override fun add(urls: List<String>, optsJson: String) {}
        override fun pause(id: ULong) {}
        override fun resume(id: ULong) {}
        override fun retry(id: ULong) {}
        override fun remove(id: ULong, active: Boolean) {}
        override fun reorder(ids: List<ULong>) {}
        override fun pauseAll() {}
        override fun resumeAll() {}
        override fun clearDone() {}
    }

    @Test
    fun download() = shot("$name-download.png") {
        DownloadContent(
            jobs = jobs, ready = "", allFiles = true, presets = emptyList(), actions = none,
            nav = { MainNav(0, 2) {} }, onAllFiles = {}, initialText = "https://youtu.be/dQw4w9WgXcQ",
        )
    }

    @Test
    fun settings() = shot("$name-settings.png") {
        SettingsContent(
            settings,
            SettingsEnv(ytdlp = "2026.10.31", vkLogged = true, hasCookies = false, battery = false, version = "Very Good Downloader для Android 0.4.0 · ядро 1.11.0"),
            object : SettingsHandler {},
            style, Palette.AUTO, {}, {},
            nav = { MainNav(3, 0) {} },
        )
    }
}
