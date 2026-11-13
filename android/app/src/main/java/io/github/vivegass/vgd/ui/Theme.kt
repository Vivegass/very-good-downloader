package io.github.vivegass.vgd.ui

import android.content.Context
import android.os.Build
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import io.github.vivegass.vgd.core.themeSchemeJson
import org.json.JSONObject

/**
 * Оформление.
 *
 * Стиль по умолчанию — по марке телефона (`Build.MANUFACTURER` / `Build.BRAND`): Pixel и прочие — Material You
 * (цвета из обоев, `dynamic*ColorScheme`), Samsung — в духе One UI, Xiaomi/Redmi/POCO — HyperOS,
 * Huawei/Honor — HarmonyOS/MagicOS; iOS-вид — только вручную. Цвет акцента у стилей марок — из системы.
 * Ниже Android 12 — встроенная тема Material 3 с ПК (тот же JSON, роли `m3.scheme`).
 * В настройках можно выбрать вручную любую тему с ПК (встроенную или файл из папки тем).
 */
object Style {
    const val AUTO = "auto"
    const val YOU = "you"
    const val ONEUI = "oneui"
    const val HYPEROS = "hyperos"
    const val HARMONY = "harmony"
    const val IOS = "ios"
    const val THEME_PREFIX = "theme:"

    /** Стили марок для выбора вручную (названия «в духе …»: без логотипов и фирменных шрифтов). */
    val BRANDS = listOf(
        ONEUI to "в духе One UI (Samsung)",
        HYPEROS to "в духе HyperOS (Xiaomi)",
        HARMONY to "в духе HarmonyOS / MagicOS (Huawei, Honor)",
        IOS to "в духе iOS (Liquid Glass)",
    )

    fun label(key: String): String = when (key) {
        YOU -> "Material You"
        else -> BRANDS.firstOrNull { it.first == key }?.second ?: key
    }

    /** Стиль по марке телефона: Pixel и остальные — Material You. */
    fun brandStyle(): String {
        val m = (Build.MANUFACTURER + " " + Build.BRAND).lowercase()
        return when {
            "google" in m -> YOU
            "samsung" in m -> ONEUI
            listOf("xiaomi", "redmi", "poco").any { it in m } -> HYPEROS
            listOf("huawei", "honor").any { it in m } -> HARMONY
            else -> YOU
        }
    }

    fun brandName(): String = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }

    fun pref(ctx: Context): String = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).getString("style", AUTO) ?: AUTO

    fun setPref(ctx: Context, v: String) {
        ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).edit().putString("style", v).apply()
    }
}

private fun parse(json: String, dark: Boolean): ColorScheme {
    val o = JSONObject(json)
    fun c(k: String, def: Color): Color =
        o.optString(k).takeIf { it.startsWith("#") }?.let { Color(android.graphics.Color.parseColor(it)) } ?: def
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = c("primary", base.primary),
        onPrimary = c("onPrimary", base.onPrimary),
        primaryContainer = c("primaryContainer", base.primaryContainer),
        onPrimaryContainer = c("onPrimaryContainer", base.onPrimaryContainer),
        inversePrimary = c("inversePrimary", base.inversePrimary),
        secondary = c("secondary", base.secondary),
        onSecondary = c("onSecondary", base.onSecondary),
        secondaryContainer = c("secondaryContainer", base.secondaryContainer),
        onSecondaryContainer = c("onSecondaryContainer", base.onSecondaryContainer),
        tertiary = c("tertiary", base.tertiary),
        onTertiary = c("onTertiary", base.onTertiary),
        tertiaryContainer = c("tertiaryContainer", base.tertiaryContainer),
        onTertiaryContainer = c("onTertiaryContainer", base.onTertiaryContainer),
        background = c("background", base.background),
        onBackground = c("onBackground", base.onBackground),
        surface = c("surface", base.surface),
        onSurface = c("onSurface", base.onSurface),
        surfaceVariant = c("surfaceVariant", base.surfaceVariant),
        onSurfaceVariant = c("onSurfaceVariant", base.onSurfaceVariant),
        surfaceTint = c("surfaceTint", base.surfaceTint),
        inverseSurface = c("inverseSurface", base.inverseSurface),
        inverseOnSurface = c("inverseOnSurface", base.inverseOnSurface),
        error = c("error", base.error),
        onError = c("onError", base.onError),
        errorContainer = c("errorContainer", base.errorContainer),
        onErrorContainer = c("onErrorContainer", base.onErrorContainer),
        outline = c("outline", base.outline),
        outlineVariant = c("outlineVariant", base.outlineVariant),
        scrim = c("scrim", base.scrim),
        surfaceBright = c("surfaceBright", base.surfaceBright),
        surfaceContainer = c("surfaceContainer", base.surfaceContainer),
        surfaceContainerHigh = c("surfaceContainerHigh", base.surfaceContainerHigh),
        surfaceContainerHighest = c("surfaceContainerHighest", base.surfaceContainerHighest),
        surfaceContainerLow = c("surfaceContainerLow", base.surfaceContainerLow),
        surfaceContainerLowest = c("surfaceContainerLowest", base.surfaceContainerLowest),
        surfaceDim = c("surfaceDim", base.surfaceDim),
    )
}

/** Тема с ПК → ColorScheme. Тёмная или светлая — по фону самой темы. */
fun schemeOfTheme(key: String, systemDark: Boolean): ColorScheme {
    val json = themeSchemeJson(key, systemDark)
    val bg = JSONObject(json).optString("background", "#000000")
    val dark = Color(android.graphics.Color.parseColor(bg)).let { 0.299f * it.red + 0.587f * it.green + 0.114f * it.blue } < 0.5f
    return parse(json, dark)
}

// ---------- стиль: форма, раскладка, материалы (поверх MaterialTheme) ----------

enum class Nav { BAR, PILL }
enum class Header { SMALL, LARGE, HUGE }

/**
 * Те же параметры, что раскладочные поля тем ПК (навигация, заголовок, карточки, фон),
 * чтобы стиль можно было потом описать JSON-файлом.
 */
data class VgdStyle(
    val key: String,
    /** BAR — панель вкладок M3; PILL — плавающая «таблетка» над содержимым (One UI, HyperOS, iOS) */
    val nav: Nav,
    /** SMALL — строка M3; LARGE — крупный заголовок слева (iOS, HyperOS); HUGE — заголовок в верхней трети (One UI) */
    val header: Header,
    val cardRadius: Dp,
    /** мягкая тень у карточек (HarmonyOS) */
    val cardElevation: Dp,
    /** «стекло»: полупрозрачные панели и карточки (размытие фона в Compose без своих шейдеров недоступно) */
    val glass: Boolean,
    /** фон экрана — чуть темнее карточек (сгруппированные списки One UI / iOS) */
    val groupedBackground: Boolean,
)

fun styleOf(key: String): VgdStyle = when (key) {
    Style.ONEUI -> VgdStyle(key, Nav.PILL, Header.HUGE, 26.dp, 0.dp, glass = false, groupedBackground = true)
    Style.HYPEROS -> VgdStyle(key, Nav.PILL, Header.LARGE, 24.dp, 0.dp, glass = true, groupedBackground = true)
    Style.HARMONY -> VgdStyle(key, Nav.BAR, Header.LARGE, 20.dp, 6.dp, glass = true, groupedBackground = false)
    Style.IOS -> VgdStyle(key, Nav.PILL, Header.LARGE, 12.dp, 0.dp, glass = true, groupedBackground = true)
    else -> VgdStyle(Style.YOU, Nav.BAR, Header.SMALL, 16.dp, 0.dp, glass = false, groupedBackground = false)
}

val LocalVgdStyle = staticCompositionLocalOf { styleOf(Style.YOU) }

/** Цвета iOS (системные: синий акцент, сгруппированный фон) — для стиля «в духе iOS». */
private fun iosScheme(dark: Boolean): ColorScheme {
    val blue = if (dark) Color(0xFF0A84FF) else Color(0xFF007AFF)
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = blue,
        onPrimary = Color.White,
        primaryContainer = blue.copy(alpha = if (dark) 0.35f else 0.16f).compositeOver(if (dark) Color.Black else Color.White),
        onPrimaryContainer = if (dark) Color.White else Color(0xFF00316B),
        secondary = blue,
        secondaryContainer = if (dark) Color(0xFF2C2C2E) else Color(0xFFE5E5EA),
        onSecondaryContainer = if (dark) Color.White else Color.Black,
        background = if (dark) Color.Black else Color(0xFFF2F2F7),
        surface = if (dark) Color.Black else Color(0xFFF2F2F7),
        surfaceContainerLowest = if (dark) Color.Black else Color.White,
        surfaceContainerLow = if (dark) Color(0xFF0E0E10) else Color(0xFFF2F2F7),
        surfaceContainer = if (dark) Color(0xFF1C1C1E) else Color.White,
        surfaceContainerHigh = if (dark) Color(0xFF2C2C2E) else Color.White,
        surfaceContainerHighest = if (dark) Color(0xFF3A3A3C) else Color(0xFFE5E5EA),
        onSurface = if (dark) Color.White else Color.Black,
        onSurfaceVariant = if (dark) Color(0xFF98989F) else Color(0xFF6C6C70),
        outline = if (dark) Color(0xFF38383A) else Color(0xFFC6C6C8),
        outlineVariant = if (dark) Color(0xFF38383A) else Color(0xFFD1D1D6),
        error = if (dark) Color(0xFFFF453A) else Color(0xFFFF3B30),
    )
}

@Composable
fun VgdTheme(style: String, content: @Composable () -> Unit) {
    val ctx = LocalContext.current
    val dark = isSystemInDarkTheme()
    val effective = if (style == Style.AUTO) Style.brandStyle() else style
    val scheme = remember(effective, dark) {
        when {
            effective.startsWith(Style.THEME_PREFIX) -> schemeOfTheme(effective.removePrefix(Style.THEME_PREFIX), dark)
            effective == Style.IOS -> iosScheme(dark)
            // Material You и стили марок: цвет акцента — из системы (Samsung, Xiaomi, Honor на Android 12+ тоже его дают)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
            // нет Material You (Android 8–11) — Material 3 с ПК
            else -> schemeOfTheme("material", dark)
        }
    }
    val vgd = styleOf(if (effective.startsWith(Style.THEME_PREFIX)) Style.YOU else effective)
    val shapes = MaterialTheme.shapes.copy(large = RoundedCornerShape(vgd.cardRadius))
    CompositionLocalProvider(LocalVgdStyle provides vgd) {
        MaterialTheme(colorScheme = scheme, shapes = shapes, content = content)
    }
}

/** Фон экрана по стилю. */
@Composable
fun screenBackground(): Color {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    return if (st.groupedBackground) c.surfaceContainerLow else c.surface
}

/** Карточка по стилю: скругление, тень, «стекло». */
@Composable
fun VgdCard(
    containerColor: Color = MaterialTheme.colorScheme.surfaceContainer,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val st = LocalVgdStyle.current
    val color = if (st.glass) containerColor.copy(alpha = 0.78f) else containerColor
    Card(
        modifier = modifier,
        shape = RoundedCornerShape(st.cardRadius),
        colors = CardDefaults.cardColors(containerColor = color),
        elevation = CardDefaults.cardElevation(defaultElevation = st.cardElevation),
        border = if (st.glass) BorderStroke(0.5.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f)) else null,
        content = content,
    )
}
