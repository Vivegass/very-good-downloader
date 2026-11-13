package io.github.vivegass.vgd.ui

import android.content.Context
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vivegass.vgd.core.themeSchemeJson
import org.json.JSONObject

/**
 * Оформление: **стиль** (раскладка, форма, шрифты, элементы) и **цвета** — отдельно.
 *
 * Стиль по умолчанию — по марке телефона: Pixel и прочие — Material You (Material 3 Expressive),
 * Samsung — One UI, Xiaomi/Redmi/POCO — HyperOS, Huawei/Honor — HarmonyOS; iOS — только вручную.
 * Стили сделаны «в духе» гайдов (Apple HIG, Samsung One UI, …): без логотипов и фирменных шрифтов —
 * шрифт системный.
 *
 * Цвета: у Material You — из обоев (Android 12+), у стилей марок — фирменный синий марки (или из обоев,
 * если выбрать), либо любая тема с ПК (тот же JSON).
 */
object Style {
    const val AUTO = "auto"
    const val YOU = "you"
    const val ONEUI = "oneui"
    const val HYPEROS = "hyperos"
    const val HARMONY = "harmony"
    const val IOS = "ios"

    val ALL = listOf(YOU, ONEUI, HYPEROS, HARMONY, IOS)

    fun label(key: String): String = when (key) {
        YOU -> "Material You"
        ONEUI -> "One UI"
        HYPEROS -> "HyperOS"
        HARMONY -> "HarmonyOS"
        IOS -> "iOS"
        else -> "По марке телефона"
    }

    fun hint(key: String): String = when (key) {
        YOU -> "Google Pixel, Material 3"
        ONEUI -> "Samsung Galaxy"
        HYPEROS -> "Xiaomi, Redmi, POCO"
        HARMONY -> "Huawei, Honor"
        IOS -> "iPhone"
        else -> ""
    }

    /** Стиль по марке телефона: Pixel и остальные — Material You. */
    fun brandStyle(): String {
        val m = (Build.MANUFACTURER + " " + Build.BRAND).lowercase()
        return when {
            "samsung" in m -> ONEUI
            listOf("xiaomi", "redmi", "poco").any { it in m } -> HYPEROS
            listOf("huawei", "honor").any { it in m } -> HARMONY
            else -> YOU
        }
    }

    fun brandName(): String = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }

    fun resolve(key: String) = if (key == AUTO || key !in ALL) brandStyle() else key

    private fun prefs(ctx: Context) = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE)

    /** Стиль; старое значение «theme:…» (0.3) — это Material You с темой с ПК. */
    fun pref(ctx: Context): String {
        val v = prefs(ctx).getString("style", AUTO) ?: AUTO
        return if (v.startsWith(Palette.THEME_PREFIX)) YOU else v
    }

    fun setPref(ctx: Context, v: String) = prefs(ctx).edit().putString("style", v).apply()

    fun colors(ctx: Context): String {
        val p = prefs(ctx)
        val old = p.getString("style", "") ?: ""
        return p.getString("colors", null) ?: if (old.startsWith(Palette.THEME_PREFIX)) old else Palette.AUTO
    }

    fun setColors(ctx: Context, v: String) = prefs(ctx).edit().putString("colors", v).apply()
}

/** Цвета: auto — у Material You из обоев, у марок фирменные; wallpaper — из обоев; brand; theme:<ключ> — тема с ПК. */
object Palette {
    const val AUTO = "auto"
    const val WALLPAPER = "wallpaper"
    const val BRAND = "brand"
    const val THEME_PREFIX = "theme:"
}

// ---------- темы с ПК → ColorScheme ----------

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

/** Тёмная ли схема (по фону). */
fun ColorScheme.isDark(): Boolean = background.let { 0.299f * it.red + 0.587f * it.green + 0.114f * it.blue } < 0.5f

// ---------- фирменные палитры марок ----------

/**
 * Палитры по образцу системных приложений: фон страницы — `background`, группы и карточки — `surfaceContainer`,
 * поля и дорожки переключателей — `surfaceContainerHighest`.
 */
fun brandScheme(style: String, dark: Boolean): ColorScheme {
    data class P(
        val accent: Color, val page: Color, val group: Color, val field: Color, val text: Color, val muted: Color,
        val line: Color, val error: Color, val tint: Color,
    )
    val p = when (style) {
        // iOS: systemBlue, systemGroupedBackground, secondarySystemGroupedBackground, separator
        Style.IOS -> if (dark) P(Color(0xFF0A84FF), Color.Black, Color(0xFF1C1C1E), Color(0xFF2C2C2E), Color.White, Color(0xFF98989F), Color(0xFF38383A), Color(0xFFFF453A), Color(0xFF0A84FF))
        else P(Color(0xFF007AFF), Color(0xFFF2F2F7), Color.White, Color(0xFFE5E5EA), Color.Black, Color(0xFF8A8A8E), Color(0xFFC6C6C8), Color(0xFFFF3B30), Color(0xFF007AFF))
        // One UI: светло-серый фон, белые «пузыри», синий Samsung
        Style.ONEUI -> if (dark) P(Color(0xFF3E91FF), Color(0xFF010101), Color(0xFF171717), Color(0xFF252525), Color(0xFFFAFAFA), Color(0xFF9A9A9A), Color(0xFF2A2A2A), Color(0xFFFF5A5A), Color(0xFF3E91FF))
        else P(Color(0xFF0381FE), Color(0xFFF2F2F2), Color(0xFFFCFCFC), Color(0xFFE6E6E6), Color(0xFF010101), Color(0xFF7A7A7A), Color(0xFFE3E3E3), Color(0xFFE83B3B), Color(0xFF0381FE))
        // HyperOS: белый плоский фон, синий Xiaomi
        Style.HYPEROS -> if (dark) P(Color(0xFF3482FF), Color.Black, Color(0xFF1A1A1A), Color(0xFF262626), Color(0xFFE6E6E6), Color(0xFF8C8C8C), Color(0xFF242424), Color(0xFFFF4D4F), Color(0xFF3482FF))
        else P(Color(0xFF0D84FF), Color.White, Color(0xFFF7F7F7), Color(0xFFF0F0F0), Color(0xFF000000), Color(0xFF999999), Color(0xFFEBEBEB), Color(0xFFF5222D), Color(0xFF0D84FF))
        // HarmonyOS: светло-серый #F1F3F5, белые карточки, синий #0A59F7
        else -> if (dark) P(Color(0xFF317AF7), Color.Black, Color(0xFF1E1E1E), Color(0xFF2C2C2C), Color(0xFFE5E5E5), Color(0xFF8E8E93), Color(0xFF333333), Color(0xFFFF4040), Color(0xFF317AF7))
        else P(Color(0xFF0A59F7), Color(0xFFF1F3F5), Color.White, Color(0xFFE5E8EB), Color(0xE6000000), Color(0x99000000), Color(0x1A000000), Color(0xFFE84026), Color(0xFF0A59F7))
    }
    val base = if (dark) darkColorScheme() else lightColorScheme()
    val tonal = p.accent.copy(alpha = if (dark) 0.24f else 0.12f)
    fun over(c: Color, bg: Color) = Color(
        red = c.red * c.alpha + bg.red * (1 - c.alpha),
        green = c.green * c.alpha + bg.green * (1 - c.alpha),
        blue = c.blue * c.alpha + bg.blue * (1 - c.alpha),
    )
    val container = over(tonal, p.group)
    return base.copy(
        primary = p.accent,
        onPrimary = Color.White,
        primaryContainer = container,
        onPrimaryContainer = p.accent,
        inversePrimary = p.accent,
        secondary = p.accent,
        onSecondary = Color.White,
        secondaryContainer = container,
        onSecondaryContainer = if (dark) Color.White else p.accent,
        tertiary = if (dark) Color(0xFF30D158) else Color(0xFF34C759),
        tertiaryContainer = over((if (dark) Color(0xFF30D158) else Color(0xFF34C759)).copy(alpha = 0.18f), p.group),
        onTertiaryContainer = p.text,
        background = p.page,
        onBackground = p.text,
        surface = p.page,
        onSurface = p.text,
        surfaceVariant = p.field,
        onSurfaceVariant = p.muted,
        surfaceTint = Color.Transparent,
        surfaceBright = p.group,
        surfaceDim = p.page,
        surfaceContainerLowest = p.group,
        surfaceContainerLow = p.group,
        surfaceContainer = p.group,
        surfaceContainerHigh = p.group,
        surfaceContainerHighest = p.field,
        outline = p.muted,
        outlineVariant = p.line,
        error = p.error,
        errorContainer = over(p.error.copy(alpha = 0.14f), p.group),
        onErrorContainer = p.error,
        inverseSurface = p.text,
        inverseOnSurface = p.page,
    )
}

// ---------- стиль: раскладка, форма, элементы ----------

enum class Kind { YOU, ONEUI, HYPER, HARMONY, IOS }

/**
 * Что отличает стили — не только скругление:
 * - заголовок: M3 large (сжимается в строку), One UI (верхняя треть экрана), iOS Large Title → навбар, HyperOS, HarmonyOS;
 * - навигация: панель M3 с индикатором, One UI (текст + жирный), iOS 26 (плавающая «стеклянная» капсула),
 *   HyperOS (плавающая панель), HarmonyOS (значки с подписью в цвет акцента);
 * - списки: сегменты M3 Expressive (большое скругление снаружи, малое между пунктами), inset grouped iOS
 *   (разделители с отступом, chevron ›), «пузыри» One UI (значение — синим), плоский список HyperOS с круглыми
 *   значками, карточки HarmonyOS с мягкой тенью;
 * - переключатели, кнопки, выбор из нескольких, прогресс — свои у каждого.
 */
data class VgdStyle(
    val key: String,
    val kind: Kind,
    /** скругление группы / карточки */
    val groupRadius: Dp,
    /** скругление между пунктами внутри группы (M3 Expressive); 0 — сплошная группа с разделителями */
    val innerRadius: Dp,
    /** промежуток между пунктами группы (M3 Expressive) */
    val itemGap: Dp,
    val thumbRadius: Dp,
    /** тень карточек (HarmonyOS) */
    val shadow: Dp,
    /** панели заголовка / навигации полупрозрачные («стекло») */
    val glass: Boolean,
    /** подпись-значение синим (One UI) */
    val summaryAccent: Boolean,
    /** › у пунктов-переходов */
    val chevron: Boolean,
    /** заголовки групп ЗАГЛАВНЫМИ (iOS) */
    val upperSection: Boolean,
    /** отступ содержимого от краёв экрана */
    val hPad: Dp,
    /** высота строки списка */
    val rowHeight: Dp,
)

fun styleOf(key: String): VgdStyle = when (key) {
    Style.ONEUI -> VgdStyle(key, Kind.ONEUI, 26.dp, 0.dp, 0.dp, 14.dp, 0.dp, glass = false, summaryAccent = true, chevron = false, upperSection = false, hPad = 12.dp, rowHeight = 64.dp)
    Style.HYPEROS -> VgdStyle(key, Kind.HYPER, 18.dp, 0.dp, 0.dp, 12.dp, 0.dp, glass = true, summaryAccent = false, chevron = true, upperSection = false, hPad = 0.dp, rowHeight = 64.dp)
    Style.HARMONY -> VgdStyle(key, Kind.HARMONY, 20.dp, 0.dp, 0.dp, 12.dp, 3.dp, glass = true, summaryAccent = false, chevron = true, upperSection = false, hPad = 16.dp, rowHeight = 56.dp)
    Style.IOS -> VgdStyle(key, Kind.IOS, 22.dp, 0.dp, 0.dp, 10.dp, 0.dp, glass = true, summaryAccent = false, chevron = true, upperSection = true, hPad = 16.dp, rowHeight = 48.dp)
    else -> VgdStyle(Style.YOU, Kind.YOU, 24.dp, 4.dp, 2.dp, 16.dp, 0.dp, glass = false, summaryAccent = false, chevron = false, upperSection = false, hPad = 16.dp, rowHeight = 72.dp)
}

val LocalVgdStyle = staticCompositionLocalOf { styleOf(Style.YOU) }

// ---------- шрифты ----------

private fun t(size: TextUnit, line: TextUnit, w: FontWeight = FontWeight.Normal, track: TextUnit = 0.sp) =
    TextStyle(fontSize = size, lineHeight = line, fontWeight = w, letterSpacing = track)

/** Шкала шрифтов стиля (роли M3 → размеры платформы). */
fun typographyOf(kind: Kind): Typography {
    val m3 = Typography()
    return when (kind) {
        // Apple HIG: Large Title 34, Title 1 28, Title 2 22, Title 3 20, Headline 17 semibold, Body 17,
        // Callout 16, Subheadline 15, Footnote 13, Caption 12/11.
        Kind.IOS -> m3.copy(
            displaySmall = t(34.sp, 41.sp, FontWeight.Bold, 0.4.sp),
            headlineLarge = t(34.sp, 41.sp, FontWeight.Bold, 0.4.sp),
            headlineMedium = t(28.sp, 34.sp, FontWeight.Bold, 0.36.sp),
            headlineSmall = t(22.sp, 28.sp, FontWeight.Bold, 0.35.sp),
            titleLarge = t(20.sp, 25.sp, FontWeight.SemiBold, 0.38.sp),
            titleMedium = t(17.sp, 22.sp, FontWeight.SemiBold, (-0.43).sp),
            titleSmall = t(15.sp, 20.sp, FontWeight.SemiBold, (-0.23).sp),
            bodyLarge = t(17.sp, 22.sp, FontWeight.Normal, (-0.43).sp),
            bodyMedium = t(15.sp, 20.sp, FontWeight.Normal, (-0.23).sp),
            bodySmall = t(13.sp, 18.sp, FontWeight.Normal, (-0.08).sp),
            labelLarge = t(17.sp, 22.sp, FontWeight.Normal, (-0.43).sp),
            labelMedium = t(13.sp, 18.sp, FontWeight.Normal, (-0.08).sp),
            labelSmall = t(11.sp, 13.sp, FontWeight.Medium, 0.06.sp),
        )
        // One UI: крупный текст, заголовок экрана лёгкий и большой, пункты 17
        Kind.ONEUI -> m3.copy(
            displaySmall = t(36.sp, 44.sp, FontWeight.Normal),
            headlineLarge = t(34.sp, 40.sp, FontWeight.Normal),
            headlineSmall = t(22.sp, 28.sp, FontWeight.Bold),
            titleLarge = t(20.sp, 26.sp, FontWeight.Bold),
            titleMedium = t(17.sp, 22.sp, FontWeight.Medium),
            titleSmall = t(15.sp, 20.sp, FontWeight.Bold),
            bodyLarge = t(17.sp, 23.sp),
            bodyMedium = t(15.sp, 20.sp),
            bodySmall = t(14.sp, 19.sp),
            labelLarge = t(16.sp, 20.sp, FontWeight.Bold),
            labelMedium = t(13.sp, 17.sp, FontWeight.Medium),
            labelSmall = t(12.sp, 15.sp, FontWeight.Medium),
        )
        // HyperOS: крупный жирный заголовок, плотные пункты 16
        Kind.HYPER -> m3.copy(
            displaySmall = t(32.sp, 40.sp, FontWeight.SemiBold),
            headlineLarge = t(30.sp, 38.sp, FontWeight.SemiBold),
            headlineSmall = t(22.sp, 28.sp, FontWeight.SemiBold),
            titleLarge = t(19.sp, 25.sp, FontWeight.SemiBold),
            titleMedium = t(16.sp, 22.sp, FontWeight.Medium),
            titleSmall = t(15.sp, 20.sp, FontWeight.SemiBold),
            bodyLarge = t(16.sp, 22.sp),
            bodyMedium = t(14.sp, 19.sp),
            bodySmall = t(13.sp, 17.sp),
            labelLarge = t(16.sp, 20.sp, FontWeight.Medium),
            labelMedium = t(12.sp, 16.sp, FontWeight.Medium),
            labelSmall = t(11.sp, 14.sp, FontWeight.Medium),
        )
        // HarmonyOS: заголовок 30 bold, пункты 16 medium, подписи 14
        Kind.HARMONY -> m3.copy(
            displaySmall = t(30.sp, 40.sp, FontWeight.Bold),
            headlineLarge = t(30.sp, 40.sp, FontWeight.Bold),
            headlineSmall = t(20.sp, 26.sp, FontWeight.Bold),
            titleLarge = t(20.sp, 26.sp, FontWeight.Bold),
            titleMedium = t(16.sp, 22.sp, FontWeight.Medium),
            titleSmall = t(14.sp, 19.sp, FontWeight.Medium),
            bodyLarge = t(16.sp, 22.sp, FontWeight.Medium),
            bodyMedium = t(14.sp, 19.sp),
            bodySmall = t(12.sp, 16.sp),
            labelLarge = t(16.sp, 21.sp, FontWeight.Medium),
            labelMedium = t(12.sp, 16.sp, FontWeight.Medium),
            labelSmall = t(10.sp, 13.sp, FontWeight.Medium),
        )
        // Material 3 (Expressive): шкала M3, заголовок экрана крупнее
        Kind.YOU -> m3.copy(headlineLarge = m3.headlineLarge.copy(fontWeight = FontWeight.Normal))
    }
}

/**
 * Тема программы. `dark` — принудительно (скриншоты); `scheme` — готовые цвета (скриншоты без системы).
 */
@Composable
fun VgdTheme(
    style: String,
    colors: String = Palette.AUTO,
    dark: Boolean? = null,
    scheme: ColorScheme? = null,
    content: @Composable () -> Unit,
) {
    val ctx = LocalContext.current
    val isDark = dark ?: isSystemInDarkTheme()
    val effective = Style.resolve(style)
    val cs = scheme ?: remember(effective, colors, isDark) {
        val dynamic = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        when {
            colors.startsWith(Palette.THEME_PREFIX) -> runCatching { schemeOfTheme(colors.removePrefix(Palette.THEME_PREFIX), isDark) }
                .getOrElse { if (isDark) darkColorScheme() else lightColorScheme() }
            effective == Style.IOS && colors != Palette.WALLPAPER -> brandScheme(Style.IOS, isDark)
            dynamic && (colors == Palette.WALLPAPER || (colors == Palette.AUTO && effective == Style.YOU)) ->
                if (isDark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
            effective == Style.YOU -> runCatching { schemeOfTheme("material", isDark) }
                .getOrElse { if (isDark) darkColorScheme() else lightColorScheme() }
            else -> brandScheme(effective, isDark)
        }
    }
    val st = styleOf(effective)
    CompositionLocalProvider(LocalVgdStyle provides st) {
        MaterialTheme(colorScheme = cs, typography = typographyOf(st.kind), content = content)
    }
}
