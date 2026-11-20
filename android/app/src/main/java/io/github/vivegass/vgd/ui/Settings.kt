@file:OptIn(ExperimentalMaterial3Api::class)

package io.github.vivegass.vgd.ui

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.PowerManager
import android.provider.DocumentsContract
import android.provider.Settings as AndroidSettings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Album
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material.icons.rounded.BatteryChargingFull
import androidx.compose.material.icons.rounded.Category
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ColorLens
import androidx.compose.material.icons.rounded.ContentCut
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.DriveFileMove
import androidx.compose.material.icons.rounded.FactCheck
import androidx.compose.material.icons.rounded.FindReplace
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.FolderSpecial
import androidx.compose.material.icons.rounded.FormatListNumbered
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.HighQuality
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Lyrics
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.People
import androidx.compose.material.icons.rounded.PlaylistPlay
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.Science
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.SlowMotionVideo
import androidx.compose.material.icons.rounded.SmartDisplay
import androidx.compose.material.icons.rounded.Subtitles
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material.icons.rounded.SystemUpdate
import androidx.compose.material.icons.rounded.ViewAgenda
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.github.vivegass.vgd.BuildConfig
import io.github.vivegass.vgd.LoginActivity
import io.github.vivegass.vgd.Vgd
import io.github.vivegass.vgd.text
import io.github.vivegass.vgd.VkAgent
import io.github.vivegass.vgd.core.MovePlan
import io.github.vivegass.vgd.core.Topic
import io.github.vivegass.vgd.core.defaultTopics
import io.github.vivegass.vgd.core.saveSettingsJson
import io.github.vivegass.vgd.core.saveTopics
import io.github.vivegass.vgd.core.settingsJson
import io.github.vivegass.vgd.core.sortApply
import io.github.vivegass.vgd.core.sortPlan
import io.github.vivegass.vgd.core.themeList
import io.github.vivegass.vgd.core.topics
import io.github.vivegass.vgd.core.updateYtdlp
import io.github.vivegass.vgd.core.yandexLoggedIn
import io.github.vivegass.vgd.core.yandexLogout
import io.github.vivegass.vgd.core.yandexPoll
import io.github.vivegass.vgd.core.yandexStart
import io.github.vivegass.vgd.core.ytdlpVersion
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

/** Что показывают настройки помимо settings.json (состояние телефона и аккаунтов). */
data class SettingsEnv(
    val ytdlp: String = "…",
    val updating: Boolean = false,
    val checking: Boolean = false,
    val vkLogged: Boolean = false,
    val yaLogged: Boolean = false,
    val yaCode: String? = null,
    val hasCookies: Boolean = false,
    val battery: Boolean = false,
    /** темы с ПК: ключ, название, цвет акцента */
    val themes: List<Triple<String, String, Color>> = emptyList(),
    val version: String = "",
    val appVersion: String = "",
    val updAuto: Boolean = true,
    val updBeta: Boolean = false,
)

/** Действия настроек (на телефоне — системные окна и ядро; в скриншот-тестах — ничего). */
interface SettingsHandler {
    fun save(change: (JSONObject) -> Unit) {}
    fun pickFolder() {}
    fun vkLogin() {}
    fun yandex() {}
    fun youtubeLogin() {}
    fun pickCookies() {}
    fun clearCookies() {}
    fun battery() {}
    fun updateYtdlp() {}
    fun check() {}
    fun sortFolder() {}
    fun topics() {}
    fun checkUpdate() {}
    fun updatePrefs(auto: Boolean, beta: Boolean) {}
}

private val HEIGHTS = listOf(0 to "Максимальное", 2160 to "2160p (4K)", 1440 to "1440p", 1080 to "1080p", 720 to "720p", 480 to "480p")
private val MUSIC = listOf("best" to "Как есть", "mp3" to "MP3 320", "m4a" to "M4A", "flac" to "FLAC")

/** Путь короче: /storage/emulated/0/Download/X → Download/X. */
private fun shortPath(p: String) = p.removePrefix(Environment.getExternalStorageDirectory().absolutePath).trimStart('/').ifEmpty { p }

/** Маленький образец стиля для листа выбора: форма карточки и цвет марки. */
@Composable
private fun StylePreview(style: String, dark: Boolean) {
    val s = styleOf(style)
    val scheme = brandScheme(if (style == Style.YOU) Style.HARMONY else style, dark)
    val accent = when (style) {
        Style.YOU -> MaterialTheme.colorScheme.primary
        else -> scheme.primary
    }
    Box(
        Modifier.size(44.dp, 30.dp).clip(RoundedCornerShape((s.groupRadius.value / 2.4f).dp))
            .background(if (style == Style.YOU) MaterialTheme.colorScheme.primaryContainer else scheme.surfaceContainer)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape((s.groupRadius.value / 2.4f).dp)),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(18.dp, 8.dp).clip(RoundedCornerShape(4.dp)).background(accent))
    }
}

@Composable
private fun Swatch(c: Color) {
    Box(Modifier.size(26.dp).clip(RoundedCornerShape(50)).background(c).border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(50)))
}

@Composable
fun SettingsContent(
    src: JSONObject?,
    env: SettingsEnv,
    h: SettingsHandler,
    style: String,
    colors: String,
    onStyle: (String) -> Unit,
    onColors: (String) -> Unit,
    nav: @Composable () -> Unit,
) {
    var rev by remember { mutableIntStateOf(0) }
    var sheet by remember { mutableStateOf<String?>(null) }
    var query by remember { mutableStateOf("") }
    // Пункты списка читают свежую копию: тот же изменённый JSONObject Compose считает «не изменился»
    // и пропускает перерисовку — тогда подпись качества и переключатели застывали на старом значении.
    val o = remember(src, rev) { src?.let { JSONObject(it.toString()) } }
    fun save(change: (JSONObject) -> Unit) {
        val obj = src ?: return
        change(obj)
        rev++
        h.save(change)
    }
    val dark = MaterialTheme.colorScheme.isDark()
    VgdScaffold(title = "Настройки", bottomBar = nav) { inner ->
        CompositionLocalProvider(LocalGroupFilter provides query) {
        LazyColumn(
            contentPadding = PaddingValues(top = inner.calculateTopPadding(), bottom = inner.calculateBottomPadding() + 24.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            item(key = "search") {
                KitTextField(
                    value = query,
                    onChange = { query = it },
                    placeholder = "Поиск по настройкам",
                    singleLine = true,
                    leading = { Icon(Icons.Rounded.Search, contentDescription = null) },
                    trailing = if (query.isEmpty()) null else ({ IconButton(onClick = { query = "" }) { Icon(Icons.Rounded.Close, contentDescription = "Очистить") } }),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                )
            }
            if (o == null) {
                item { Text("Загружаю…", modifier = Modifier.padding(24.dp)) }
                return@LazyColumn
            }
            item(key = "dl") {
                Group("Загрузки") {
                    nav("Папка", value = shortPath(o.optString("out_dir")), icon = Icons.Rounded.Folder, tint = Tint.BLUE) { h.pickFolder() }
                    nav("Качество видео", value = HEIGHTS.firstOrNull { it.first == o.optInt("max_height") }?.second ?: "${o.optInt("max_height")}p", icon = Icons.Rounded.HighQuality, tint = Tint.RED) { sheet = "height" }
                    custom("Кадров в секунду fps частота") {
                        val fps = listOf(0, 60, 30, 24)
                        Column {
                            KitRow("Кадров в секунду", subtitle = "Если такой частоты нет — пересчёт", icon = Icons.Rounded.SlowMotionVideo, tint = Tint.ORANGE)
                            KitSegmented(
                                listOf("Макс.", "60", "30", "24"),
                                fps.indexOf(o.optInt("max_fps")).coerceAtLeast(0),
                                { i -> save { it.put("max_fps", fps[i]) } },
                                Modifier.padding(start = rowPad(), end = rowPad(), bottom = 12.dp),
                            )
                        }
                    }
                    toggle("Раскладывать по папкам", o.optBoolean("sort"), "Тема / Канал", Icons.Rounded.FolderSpecial, Tint.ORANGE) { v -> save { it.put("sort", v) } }
                    toggle("Плейлисты и каналы целиком", o.optBoolean("playlist", true), icon = Icons.Rounded.PlaylistPlay, tint = Tint.PURPLE) { v -> save { it.put("playlist", v) } }
                    toggle("Субтитры", o.optBoolean("subs"), o.optString("sub_langs", "ru,en").replace(",", ", "), Icons.Rounded.Subtitles, Tint.GRAY) { v -> save { it.put("subs", v) } }
                    toggle("Обложка рядом с видео", o.optBoolean("thumbnail", true), icon = Icons.Rounded.Image, tint = Tint.TEAL) { v -> save { it.put("thumbnail", v) } }
                    toggle("Вырезать спонсорские вставки", o.optBoolean("sponsorblock"), "SponsorBlock", Icons.Rounded.ContentCut, Tint.GREEN) { v -> save { it.put("sponsorblock", v) } }
                    toggle("Разбивать по главам", o.optBoolean("split_chapters"), icon = Icons.Rounded.ViewAgenda, tint = Tint.INDIGO) { v -> save { it.put("split_chapters", v) } }
                    toggle("Файл «_откуда скачано.txt»", o.optBoolean("source_log", true), "Ссылки рядом с файлами", Icons.Rounded.Description, Tint.GRAY) { v -> save { it.put("source_log", v) } }
                }
            }
            item(key = "edit") {
                val h264 = o.optString("edit", "none") == "h264"
                Group("Копия для монтажа", footer = "H.264 MP4 + AAC для After Effects и Premiere: VP9, AV1 и Opus они не открывают. ProRes на телефоне не делаем — медленно, телефон греется.") {
                    toggle("Копия H.264", h264, icon = Icons.Rounded.Movie, tint = Tint.PINK) { v -> save { it.put("edit", if (v) "h264" else "none") } }
                    toggle("Вместо оригинала", o.optBoolean("edit_replace"), "MKV не остаётся", Icons.Rounded.SwapHoriz, Tint.ORANGE, enabled = h264) { v -> save { it.put("edit_replace", v) } }
                }
            }
            item(key = "music") {
                val replace = !o.optBoolean("music_no_replace")
                Group("Музыка") {
                    nav("Формат", value = MUSIC.firstOrNull { it.first == o.optString("music_format", "best") }?.second ?: "Как есть", icon = Icons.Rounded.MusicNote, tint = Tint.PINK) { sheet = "music" }
                    toggle("Номер трека в имени файла", o.optBoolean("music_numbers", true), icon = Icons.Rounded.FormatListNumbered, tint = Tint.BLUE) { v -> save { it.put("music_numbers", v) } }
                    toggle("Искать замену недоступным", replace, "На YouTube Music, SoundCloud и других", Icons.Rounded.FindReplace, Tint.ORANGE) { v -> save { it.put("music_no_replace", !v) } }
                    toggle("Замены — в отдельную папку", o.optBoolean("music_replace_dir"), "«_из других мест»", Icons.Rounded.DriveFileMove, Tint.GRAY, enabled = replace) { v -> save { it.put("music_replace_dir", v) } }
                    toggle("Проверять замену по звуку", o.optBoolean("music_verify", true), "Сравнение с образцом Deezer", Icons.Rounded.GraphicEq, Tint.PURPLE) { v -> save { it.put("music_verify", v) } }
                    toggle("Тексты песен", o.optBoolean("lyrics"), "LRCLIB, с таймингом — .lrc рядом", Icons.Rounded.Lyrics, Tint.TEAL) { v -> save { it.put("lyrics", v) } }
                }
            }
            item(key = "acc") {
                Group("Аккаунты", footer = "Пароли программа не видит: вход — на страницах самих сервисов.") {
                    nav("VK", value = if (env.vkLogged) "Вход выполнен" else "Войти", icon = Icons.Rounded.People, tint = Tint.BLUE) { h.vkLogin() }
                    toggle(
                        "Искать музыку в VK первым",
                        o.optBoolean("vk_search") && env.vkLogged,
                        if (env.vkLogged) "Звук прямо из VK" else "Сначала войдите в VK",
                        enabled = env.vkLogged,
                    ) { v -> save { it.put("vk_search", v) } }
                    nav(
                        "Яндекс Музыка",
                        value = when {
                            env.yaCode != null -> "Код ${env.yaCode}"
                            env.yaLogged -> "Вход выполнен"
                            else -> "Войти по коду"
                        },
                        subtitle = env.yaCode?.let { "Введите код на ya.ru/device" },
                        icon = Icons.Rounded.Album,
                        tint = Tint.YELLOW,
                    ) { h.yandex() }
                    if (env.yaLogged) {
                        toggle("Звук прямо с Яндекса", o.optBoolean("yandex_acc"), "FLAC и поиск замены там") { v -> save { it.put("yandex_acc", v) } }
                    }
                    nav("YouTube", value = if (env.hasCookies) "Куки есть" else "Без входа", subtitle = "Для 18+ и закрытых видео", icon = Icons.Rounded.SmartDisplay, tint = Tint.RED) { sheet = "youtube" }
                }
            }
            item(key = "folder") {
                Group("Папка загрузок") {
                    nav("Разложить папку", subtitle = "По «Тема / Канал» — сначала покажу план", icon = Icons.Rounded.AutoFixHigh, tint = Tint.INDIGO) { h.sortFolder() }
                    nav("Категории", subtitle = "Ключевые слова тем", icon = Icons.Rounded.Category, tint = Tint.ORANGE) { h.topics() }
                }
            }
            item(key = "look") {
                val colorsLabel = when {
                    colors == Palette.WALLPAPER -> "Из обоев"
                    colors.startsWith(Palette.THEME_PREFIX) -> env.themes.firstOrNull { it.first == colors.removePrefix(Palette.THEME_PREFIX) }?.second ?: "Тема с ПК"
                    else -> "Как в стиле"
                }
                Group("Оформление") {
                    nav(
                        "Стиль",
                        value = if (style == Style.AUTO) "По марке (${Style.label(Style.brandStyle())})" else Style.label(style),
                        icon = Icons.Rounded.Palette,
                        tint = Tint.PURPLE,
                    ) { sheet = "style" }
                    nav("Цвета", value = colorsLabel, icon = Icons.Rounded.ColorLens, tint = Tint.PINK) { sheet = "colors" }
                }
            }
            item(key = "phone") {
                Group("Телефон") {
                    nav(
                        "Не оптимизировать батарею",
                        value = if (env.battery) "Готово" else null,
                        subtitle = if (env.battery) "Система не остановит долгие загрузки" else "Чтобы система не останавливала долгие загрузки",
                        icon = Icons.Rounded.BatteryChargingFull,
                        tint = Tint.GREEN,
                        enabled = !env.battery,
                    ) { h.battery() }
                }
            }
            item(key = "update") {
                val ust by io.github.vivegass.vgd.Updater.state.collectAsState()
                Group("Обновления", footer = "С GitHub (общий релиз с ПК), файл сверяется по SHA-256. Ставится поверх, если APK подписан тем же ключом.") {
                    nav(
                        "Проверить обновления",
                        value = when (val u = ust) {
                            is io.github.vivegass.vgd.Updater.State.Checking -> "Проверяю…"
                            is io.github.vivegass.vgd.Updater.State.Latest -> "Последняя версия"
                            is io.github.vivegass.vgd.Updater.State.Found -> "Есть ${u.info.version}"
                            is io.github.vivegass.vgd.Updater.State.Downloading -> "Скачиваю…"
                            is io.github.vivegass.vgd.Updater.State.Error -> "Ошибка"
                            else -> "Версия ${env.appVersion}"
                        },
                        subtitle = (ust as? io.github.vivegass.vgd.Updater.State.Error)?.message,
                        icon = Icons.Rounded.SystemUpdate,
                        tint = Tint.BLUE,
                    ) { h.checkUpdate() }
                    toggle("Проверять автоматически", env.updAuto, "Раз в 6 часов", icon = Icons.Rounded.Schedule, tint = Tint.GREEN) { h.updatePrefs(it, env.updBeta) }
                    toggle("Бета-версии", env.updBeta, "Предлагать предварительные выпуски", icon = Icons.Rounded.Science, tint = Tint.ORANGE) { h.updatePrefs(env.updAuto, it) }
                }
            }
            item(key = "ytdlp") {
                Group("yt-dlp", footer = "YouTube регулярно ломает старые версии yt-dlp — если видео перестали качаться, обновите.") {
                    nav(if (env.updating) "Обновляю…" else "Обновить yt-dlp", value = env.ytdlp, icon = Icons.Rounded.SystemUpdate, tint = Tint.BLUE, enabled = !env.updating) { h.updateYtdlp() }
                    nav(if (env.checking) "Проверяю…" else "Проверка", subtitle = "yt-dlp, ffmpeg, форматы YouTube — результат в «Журнале»", icon = Icons.Rounded.FactCheck, tint = Tint.GREEN, enabled = !env.checking) { h.check() }
                }
            }
            if (query.isEmpty()) item(key = "ver") {
                Text(
                    env.version,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp),
                )
            }
        }
        }
    }

    // ---------- листы выбора ----------
    val obj = o ?: return
    when (sheet) {
        "height" -> ChoiceSheet(
            "Качество видео",
            HEIGHTS.map { Choice(it.first.toString(), it.second, if (it.first == 0) "Лучшее, что есть у ролика" else null) },
            obj.optInt("max_height").toString(),
            onSelect = { k -> save { it.put("max_height", k.toInt()) }; sheet = null },
            onDismiss = { sheet = null },
        )
        "music" -> ChoiceSheet(
            "Формат музыки",
            MUSIC.map { Choice(it.first, it.second, if (it.first == "best") "Без перекодирования (обычно Opus или M4A)" else null) },
            obj.optString("music_format", "best"),
            onSelect = { k -> save { it.put("music_format", k) }; sheet = null },
            onDismiss = { sheet = null },
        )
        "style" -> ChoiceSheet(
            "Стиль",
            listOf(Choice(Style.AUTO, "По марке телефона", "${Style.brandName()}: ${Style.label(Style.brandStyle())}")) +
                Style.ALL.map { k -> Choice(k, Style.label(k), Style.hint(k)) { StylePreview(k, dark) } },
            style,
            onSelect = { onStyle(it); sheet = null },
            onDismiss = { sheet = null },
        )
        "colors" -> ChoiceSheet(
            "Цвета",
            buildList {
                add(Choice(Palette.AUTO, "Как в стиле", "Material You — из обоев, у марок — фирменные"))
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(Choice(Palette.WALLPAPER, "Из обоев", "Material You для любого стиля"))
                env.themes.forEach { (k, name, c) -> add(Choice(Palette.THEME_PREFIX + k, name, "Тема с ПК") { Swatch(c) }) }
            },
            colors,
            onSelect = { onColors(it); sheet = null },
            onDismiss = { sheet = null },
        )
        "youtube" -> ChoiceSheet(
            "YouTube",
            buildList {
                add(Choice("login", "Войти в Google", "Во встроенном браузере; Google может не пустить"))
                add(Choice("file", "Выбрать cookies.txt", "Выгрузите на ПК расширением «Get cookies.txt»"))
                if (env.hasCookies) add(Choice("clear", "Убрать куки"))
            },
            "",
            onSelect = { k ->
                sheet = null
                when (k) {
                    "login" -> h.youtubeLogin()
                    "file" -> h.pickCookies()
                    "clear" -> h.clearCookies()
                }
            },
            onDismiss = { sheet = null },
        )
    }
}

// ---------- на телефоне: данные и системные окна ----------

@Composable
internal fun SettingsScreen(nav: @Composable () -> Unit, style: String, colors: String, onStyle: (String) -> Unit, onColors: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var json by remember { mutableStateOf<JSONObject?>(null) }
    var env by remember { mutableStateOf(SettingsEnv(vkLogged = VkAgent.loggedIn(), hasCookies = LoginActivity.cookiesFile(ctx).isFile, battery = ignoresBattery(ctx))) }
    var plan by remember { mutableStateOf<List<MovePlan>?>(null) }
    var topicsOpen by remember { mutableStateOf(false) }
    val dark = MaterialTheme.colorScheme.isDark()
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            try {
                Vgd.core()
                json = JSONObject(settingsJson())
                val themes = runCatching {
                    themeList().map { t -> Triple(t.key, t.name, runCatching { schemeOfTheme(t.key, dark).primary }.getOrDefault(Color.Gray)) }
                }.getOrDefault(emptyList())
                val up = runCatching { io.github.vivegass.vgd.core.updatePrefs() }.getOrNull()
                env = env.copy(
                    appVersion = BuildConfig.VERSION_NAME,
                    updAuto = up?.auto ?: true,
                    updBeta = up?.beta ?: false,
                    ytdlp = ytdlpVersion() ?: "не найден",
                    yaLogged = runCatching { yandexLoggedIn() }.getOrDefault(false),
                    themes = themes,
                    version = "Very Good Downloader для Android ${BuildConfig.VERSION_NAME} · ядро ${io.github.vivegass.vgd.core.version()}\n${ctx.packageName}",
                )
            } catch (e: Throwable) {
                env = env.copy(ytdlp = "ошибка: ${e.text()}")
            }
        }
    }
    LifecycleResumeEffect(Unit) {
        env = env.copy(battery = ignoresBattery(ctx), vkLogged = VkAgent.loggedIn(), hasCookies = LoginActivity.cookiesFile(ctx).isFile)
        onPauseOrDispose { }
    }
    val cookies = LoginActivity.cookiesFile(ctx)
    fun saveNow(change: (JSONObject) -> Unit) {
        val o = json ?: return
        change(o)
        scope.launch(Dispatchers.IO) {
            try {
                saveSettingsJson(o.toString())
                Vgd.coreOrNull()?.reloadSettings()
            } catch (e: Throwable) {
                Vgd.addLog("Ошибка: настройки: ${e.text()}")
            }
        }
    }
    fun useCookies() = saveNow {
        it.put("cookies", "file")
        it.put("cookies_file", cookies.absolutePath)
    }
    val pickFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        val p = uri?.let { treeToPath(it) } ?: return@rememberLauncherForActivityResult
        saveNow { it.put("out_dir", p) }
    }
    val vkLogin = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { env = env.copy(vkLogged = VkAgent.loggedIn()) }
    val ytLogin = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        env = env.copy(hasCookies = cookies.isFile)
        if (cookies.isFile) useCookies()
    }
    val pickCookies = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        try {
            cookies.parentFile?.mkdirs()
            ctx.contentResolver.openInputStream(uri)?.use { inp -> cookies.outputStream().use { inp.copyTo(it) } }
            env = env.copy(hasCookies = true)
            useCookies()
            Vgd.addLog("YouTube: cookies.txt загружен")
        } catch (e: Exception) {
            Vgd.addLog("Ошибка: cookies.txt: ${e.text()}")
        }
    }
    val handler = object : SettingsHandler {
        override fun save(change: (JSONObject) -> Unit) = saveNow {}
        override fun pickFolder() = pickFolder.launch(null)
        override fun vkLogin() = vkLogin.launch(LoginActivity.intent(ctx, LoginActivity.VK))
        override fun youtubeLogin() = ytLogin.launch(LoginActivity.intent(ctx, LoginActivity.YOUTUBE))
        override fun pickCookies() = pickCookies.launch(arrayOf("text/plain", "*/*"))
        override fun clearCookies() {
            cookies.delete()
            env = env.copy(hasCookies = false)
            saveNow { it.put("cookies", "none") }
        }

        @SuppressLint("BatteryLife")
        override fun battery() {
            try {
                ctx.startActivity(Intent(AndroidSettings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + ctx.packageName)))
            } catch (_: Exception) {
                ctx.startActivity(Intent(AndroidSettings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
            }
        }

        override fun yandex() {
            if (env.yaLogged) {
                yandexLogout()
                env = env.copy(yaLogged = false)
                saveNow { it.put("yandex_acc", false) }
                return
            }
            if (env.yaCode != null) return
            scope.launch(Dispatchers.IO) {
                try {
                    val d = yandexStart()
                    env = env.copy(yaCode = d.userCode)
                    withContext(Dispatchers.Main) { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(d.url))) } }
                    val end = System.currentTimeMillis() + d.expiresIn.toLong() * 1000
                    while (System.currentTimeMillis() < end) {
                        delay(d.interval.toLong() * 1000)
                        if (yandexPoll(d.deviceCode)) {
                            env = env.copy(yaLogged = true)
                            break
                        }
                    }
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: вход в Яндекс: ${e.text()}")
                }
                env = env.copy(yaCode = null)
                if (env.yaLogged) saveNow { it.put("yandex_acc", true) }
            }
        }

        override fun updateYtdlp() {
            env = env.copy(updating = true)
            scope.launch(Dispatchers.IO) {
                val v = try {
                    io.github.vivegass.vgd.core.updateYtdlp()
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: обновление yt-dlp: ${e.text()}")
                    ytdlpVersion() ?: "?"
                }
                env = env.copy(updating = false, ytdlp = v)
            }
        }

        override fun check() {
            env = env.copy(checking = true)
            scope.launch(Dispatchers.IO) {
                Vgd.addLog("Проверка: ролик 240p (Me at the zoo) и ролик 4K (Big Buck Bunny)…")
                try {
                    Vgd.core()
                    for (url in listOf("https://www.youtube.com/watch?v=jNQXAC9IVRw", "https://www.youtube.com/watch?v=aqz-KE-bpKQ")) {
                        Vgd.addLog("— $url")
                        io.github.vivegass.vgd.core.diagnose(url).forEach { Vgd.addLog(it) }
                    }
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: проверка: ${e.text()}")
                }
                env = env.copy(checking = false)
            }
        }

        override fun sortFolder() {
            val root = json?.optString("out_dir") ?: return
            scope.launch(Dispatchers.IO) {
                plan = try {
                    Vgd.core()
                    sortPlan(root).ifEmpty {
                        Vgd.addLog("Разложить: в корне папки нет неразложенных видео, скачанных программой.")
                        null
                    }
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: разложить: ${e.text()}")
                    null
                }
            }
        }

        override fun topics() {
            topicsOpen = true
        }

        override fun checkUpdate() = io.github.vivegass.vgd.Updater.check(ctx, manual = true)

        override fun updatePrefs(auto: Boolean, beta: Boolean) {
            env = env.copy(updAuto = auto, updBeta = beta)
            scope.launch(Dispatchers.IO) { io.github.vivegass.vgd.core.setUpdatePrefs(auto, beta) }
            io.github.vivegass.vgd.Updater.schedule(ctx, auto)
        }
    }
    SettingsContent(json, env, handler, style, colors, onStyle, onColors, nav)
    plan?.let { p -> SortDialog(p, json?.optString("out_dir").orEmpty()) { plan = null } }
    if (topicsOpen) TopicsDialog { topicsOpen = false }
}

// ---------- папка: системный выбор ----------

/** content://…/tree/primary:Download/X → /storage/emulated/0/Download/X (с «Доступом ко всем файлам» — обычный путь). */
internal fun treeToPath(uri: Uri): String? {
    val id = try { DocumentsContract.getTreeDocumentId(uri) } catch (_: Exception) { return null }
    val (vol, rel) = id.split(':', limit = 2).let { it[0] to it.getOrElse(1) { "" } }
    val root = if (vol.equals("primary", ignoreCase = true)) Environment.getExternalStorageDirectory().absolutePath else "/storage/$vol"
    return if (rel.isEmpty()) root else "$root/$rel"
}

internal fun ignoresBattery(ctx: Context): Boolean =
    (ctx.getSystemService(Context.POWER_SERVICE) as PowerManager).isIgnoringBatteryOptimizations(ctx.packageName)

// ---------- «Разложить папку» ----------

@Composable
private fun SortDialog(p: List<MovePlan>, root: String, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Разложить ${p.size} файл(ов)?") },
        text = {
            Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
                p.take(200).forEach { m ->
                    Text(File(m.from).name + " → " + m.to.removePrefix(root).trimStart('/'), style = MaterialTheme.typography.bodySmall)
                }
            }
        },
        confirmButton = {
            KitButton("Разложить", {
                onClose()
                scope.launch(Dispatchers.IO) { Vgd.addLog("Разложено файлов: ${sortApply(p)}") }
            }, kind = Btn.TEXT)
        },
        dismissButton = { KitButton("Отмена", onClose, kind = Btn.TEXT) },
    )
}

// ---------- категории (topics.json) ----------

@Composable
private fun TopicsDialog(onClose: () -> Unit) {
    var rows by remember { mutableStateOf(runCatching { topics() }.getOrDefault(emptyList()).map { it.name to it.keywords.joinToString(", ") }) }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Категории") },
        text = {
            Column(Modifier.heightIn(max = 460.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Тема видео — по ключевым словам (название ×3, плейлист ×4, теги, ник канала). Выше — важнее.", style = MaterialTheme.typography.bodySmall)
                rows.forEachIndexed { i, (name, kw) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            KitTextField(name, { v -> rows = rows.toMutableList().also { it[i] = v to kw } }, "Название", singleLine = true)
                            KitTextField(kw, { v -> rows = rows.toMutableList().also { it[i] = name to v } }, "Ключевые слова через запятую")
                        }
                        IconButton(onClick = { rows = rows.toMutableList().also { it.removeAt(i) } }) { Icon(Icons.Rounded.Close, contentDescription = "Убрать") }
                    }
                }
                Row {
                    KitButton("Добавить", { rows = rows + ("" to "") }, kind = Btn.TEXT)
                    KitButton("Как было", { rows = defaultTopics().map { it.name to it.keywords.joinToString(", ") } }, kind = Btn.TEXT)
                }
            }
        },
        confirmButton = {
            KitButton("Сохранить", {
                try {
                    saveTopics(rows.map { (n, k) -> Topic(n, k.split(',').map { it.trim() }.filter { it.isNotEmpty() }) })
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: категории: ${e.text()}")
                }
                onClose()
            }, kind = Btn.TEXT)
        },
        dismissButton = { KitButton("Отмена", onClose, kind = Btn.TEXT) },
    )
}

/** Строка-переключатель вне группы (подписки). */
@Composable
internal fun Toggle(label: String, value: Boolean, onChange: (Boolean) -> Unit) {
    var v by remember(value) { mutableStateOf(value) }
    KitRow(label, onClick = { v = !v; onChange(v) }) { KitSwitch(v) }
}
