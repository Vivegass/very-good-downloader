package io.github.vivegass.vgd.ui

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Link
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.QueueMusic
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/** Откуда ссылка: название для строки статуса, цвет и значок заглушки превью. */
data class Platform(val name: String, val color: Color, val icon: ImageVector, val music: Boolean)

fun platformOf(url: String, audio: Boolean = false): Platform {
    val u = url.lowercase()
    fun p(n: String, c: Long, music: Boolean, icon: ImageVector = if (music) Icons.Rounded.MusicNote else Icons.Rounded.PlayArrow) =
        Platform(n, Color(c), icon, music)
    return when {
        "music.youtube.com" in u -> p("YouTube Music", 0xFFFF0033, true)
        "youtube.com" in u || "youtu.be" in u -> p("YouTube", 0xFFFF0033, audio)
        "soundcloud.com" in u -> p("SoundCloud", 0xFFFF5500, true, Icons.Rounded.GraphicEq)
        "vk.com" in u || "vk.ru" in u || "vkvideo" in u -> p("VK", 0xFF0077FF, "audio" in u || audio)
        "spotify" in u -> p("Spotify", 0xFF1DB954, true)
        "music.yandex" in u -> p("Яндекс Музыка", 0xFFFFBC0D, true)
        "music.apple.com" in u -> p("Apple Music", 0xFFFA2D48, true)
        "deezer.com" in u -> p("Deezer", 0xFFA238FF, true)
        "tiktok.com" in u -> p("TikTok", 0xFF111111, audio)
        "instagram.com" in u -> p("Instagram", 0xFFE1306C, audio)
        "twitter.com" in u || "x.com/" in u -> p("X", 0xFF111111, audio)
        "vimeo.com" in u -> p("Vimeo", 0xFF1AB7EA, audio)
        "rutube.ru" in u -> p("RUTUBE", 0xFF100943, audio)
        "twitch.tv" in u -> p("Twitch", 0xFF9146FF, audio)
        u.startsWith("vgdlist:") -> p("Список", 0xFF5E5CE6, true, Icons.Rounded.QueueMusic)
        else -> p("Ссылка", 0xFF6E6E73, audio, if (audio) Icons.Rounded.MusicNote else Icons.Rounded.Link)
    }
}

/** id ролика YouTube из ссылки — превью видно сразу, ещё в очереди. */
fun youtubeId(url: String): String? {
    val m = Regex("""(?:youtu\.be/|[?&]v=|/shorts/|/live/|/embed/)([A-Za-z0-9_-]{11})""").find(url) ?: return null
    return m.groupValues[1]
}

/** Превью: от ядра, иначе для YouTube — по id из ссылки. */
fun thumbFor(url: String, thumb: String): String? =
    thumb.ifEmpty { null } ?: youtubeId(url)?.let { "https://i.ytimg.com/vi/$it/mqdefault.jpg" }

/** Загрузчик картинок (подменяется в скриншот-тестах). */
fun interface ThumbLoader {
    suspend fun load(url: String): ImageBitmap?
}

/** Свой маленький загрузчик: HTTP + уменьшение до ~360 px + кеш в памяти. Без сторонних библиотек. */
object NetThumbs : ThumbLoader {
    private val cache = LruCache<String, ImageBitmap>(80)

    fun cached(url: String): ImageBitmap? = cache.get(url)

    override suspend fun load(url: String): ImageBitmap? = cache.get(url) ?: withContext(Dispatchers.IO) {
        runCatching {
            val c = URL(url).openConnection() as HttpURLConnection
            c.connectTimeout = 10_000
            c.readTimeout = 15_000
            c.instanceFollowRedirects = true
            val bytes = c.inputStream.use { it.readBytes() }
            val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, o)
            var sample = 1
            while (o.outWidth / (sample * 2) >= 360) sample *= 2
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
        }.getOrNull()?.also { cache.put(url, it) }
    }
}

val LocalThumbs = staticCompositionLocalOf<ThumbLoader> { NetThumbs }

/**
 * Превью карточки: картинка, пока её нет — заглушка цвета платформы со значком.
 */
@Composable
fun Thumb(url: String?, platform: Platform, width: Dp, height: Dp, radius: Dp, modifier: Modifier = Modifier) {
    val loader = LocalThumbs.current
    val img by produceState<ImageBitmap?>(url?.let { NetThumbs.cached(it) }, url, loader) {
        if (value == null && url != null) value = loader.load(url)
    }
    val shape = RoundedCornerShape(radius)
    Box(modifier.size(width, height).clip(shape), contentAlignment = Alignment.Center) {
        val i = img
        if (i != null) {
            Image(i, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Box(
                Modifier.fillMaxSize().background(
                    Brush.linearGradient(listOf(platform.color, platform.color.copy(alpha = 0.72f).compositeOverBlack())),
                ),
                contentAlignment = Alignment.Center,
            ) {
                Icon(platform.icon, contentDescription = platform.name, tint = Color.White, modifier = Modifier.size(height * 0.5f))
            }
        }
    }
}

private fun Color.compositeOverBlack() = Color(red * alpha, green * alpha, blue * alpha)
