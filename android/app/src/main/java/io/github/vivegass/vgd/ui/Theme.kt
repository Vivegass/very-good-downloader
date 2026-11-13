package io.github.vivegass.vgd.ui

import android.content.Context
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
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
 * Стиль по умолчанию — по марке телефона (`Build.MANUFACTURER` / `Build.BRAND`): на Pixel и любом Android 12+
 * — Material You (цвета из обоев, `dynamic*ColorScheme`). Стили марок (Samsung — One UI, Xiaomi — HyperOS,
 * Huawei/Honor — HarmonyOS/MagicOS, iOS-вид) — этап 6; до тех пор все марки получают Material You.
 * Ниже Android 12 — встроенная тема Material 3 с ПК (тот же JSON, роли `m3.scheme`).
 * В настройках можно выбрать вручную любую тему с ПК (встроенную или файл из папки тем).
 */
object Style {
    const val AUTO = "auto"
    const val YOU = "you"
    const val THEME_PREFIX = "theme:"

    /** Стиль марки (этап 6 подставит свои; сейчас — Material You у всех). */
    fun brandStyle(): String {
        val m = (Build.MANUFACTURER + " " + Build.BRAND).lowercase()
        return when {
            "google" in m -> YOU
            // "samsung" → One UI, "xiaomi"/"redmi"/"poco" → HyperOS, "huawei"/"honor" → HarmonyOS — этап 6
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

@Composable
fun VgdTheme(style: String, content: @Composable () -> Unit) {
    val ctx = LocalContext.current
    val dark = isSystemInDarkTheme()
    val effective = if (style == Style.AUTO) Style.brandStyle() else style
    val scheme = remember(effective, dark) {
        when {
            effective.startsWith(Style.THEME_PREFIX) -> schemeOfTheme(effective.removePrefix(Style.THEME_PREFIX), dark)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
            // нет Material You (Android 8–11) — Material 3 с ПК
            else -> schemeOfTheme("material", dark)
        }
    }
    MaterialTheme(colorScheme = scheme, content = content)
}
