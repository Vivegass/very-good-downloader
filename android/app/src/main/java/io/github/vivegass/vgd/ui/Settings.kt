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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.github.vivegass.vgd.BuildConfig
import io.github.vivegass.vgd.LoginActivity
import io.github.vivegass.vgd.Vgd
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

/** Настройки — тот же settings.json, что на ПК (поля по именам из rust/core/src/settings.rs). */
@Composable
internal fun SettingsPage(pad: PaddingValues, style: String, onStyle: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var json by remember { mutableStateOf<JSONObject?>(null) }
    var rev by remember { mutableIntStateOf(0) }
    var ytdlp by remember { mutableStateOf("…") }
    var updating by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            try {
                Vgd.core()
                json = JSONObject(settingsJson())
                ytdlp = ytdlpVersion() ?: "не найден"
            } catch (e: Throwable) {
                ytdlp = "ошибка: ${e.message}"
            }
        }
    }
    fun save(change: (JSONObject) -> Unit) {
        val o = json ?: return
        change(o)
        rev++
        scope.launch(Dispatchers.IO) {
            try {
                saveSettingsJson(o.toString())
                Vgd.coreOrNull()?.reloadSettings()
            } catch (e: Throwable) {
                Vgd.addLog("Ошибка: настройки: ${e.message}")
            }
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, top = pad.calculateTopPadding(), bottom = pad.calculateBottomPadding() + 16.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val o = json
        @Suppress("UNUSED_EXPRESSION") rev
        if (o == null) {
            Text("загружаю…")
            return@Column
        }

        // ---------- загрузки ----------
        Section("Загрузки")
        FolderRow(o.optString("out_dir")) { dir -> save { it.put("out_dir", dir) } }
        Text("разрешение", style = MaterialTheme.typography.labelLarge)
        Chips(listOf(0 to "макс", 2160 to "2160p", 1440 to "1440p", 1080 to "1080p", 720 to "720p", 480 to "480p"), o.optInt("max_height")) { v ->
            save { it.put("max_height", v) }
        }
        Text("кадров в секунду (если такого нет — пересчёт)", style = MaterialTheme.typography.labelLarge)
        Chips(listOf(0 to "макс", 60 to "60", 30 to "30", 24 to "24"), o.optInt("max_fps")) { v -> save { it.put("max_fps", v) } }
        Toggle("раскладывать по папкам «Тема / Канал»", o.optBoolean("sort")) { v -> save { it.put("sort", v) } }
        Toggle("плейлисты и каналы целиком", o.optBoolean("playlist", true)) { v -> save { it.put("playlist", v) } }
        Toggle("субтитры (${o.optString("sub_langs", "ru,en")})", o.optBoolean("subs")) { v -> save { it.put("subs", v) } }
        Toggle("обложка .jpg рядом с видео", o.optBoolean("thumbnail", true)) { v -> save { it.put("thumbnail", v) } }
        Toggle("вырезать спонсорские вставки (SponsorBlock)", o.optBoolean("sponsorblock")) { v -> save { it.put("sponsorblock", v) } }
        Toggle("разбивать видео по главам", o.optBoolean("split_chapters")) { v -> save { it.put("split_chapters", v) } }
        Toggle("«_откуда скачано.txt» рядом с файлами", o.optBoolean("source_log", true)) { v -> save { it.put("source_log", v) } }

        // ---------- копия для монтажа ----------
        Section("Копия для монтажа")
        Text(
            "H.264 MP4 + AAC для After Effects / Premiere (VP9/AV1/Opus они не открывают). На телефоне — только H.264 " +
                "(ProRes скрыт: медленно, телефон греется).",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        val edit = if (o.optString("edit", "none") == "h264") 1 else 0
        Chips(listOf(0 to "не делать", 1 to "H.264"), edit) { v -> save { it.put("edit", if (v == 1) "h264" else "none") } }
        if (edit == 1) {
            Toggle("сохранять копию вместо оригинала (MKV не остаётся)", o.optBoolean("edit_replace")) { v -> save { it.put("edit_replace", v) } }
        }

        // ---------- музыка ----------
        Section("Музыка")
        Text("формат", style = MaterialTheme.typography.labelLarge)
        val formats = listOf("best", "mp3", "m4a", "flac")
        Chips(listOf(0 to "как есть", 1 to "mp3 320", 2 to "m4a", 3 to "flac"), formats.indexOf(o.optString("music_format", "best")).coerceAtLeast(0)) { v ->
            save { it.put("music_format", formats[v]) }
        }
        Toggle("номер трека в имени файла", o.optBoolean("music_numbers", true)) { v -> save { it.put("music_numbers", v) } }
        Toggle("искать замену недоступным трекам", !o.optBoolean("music_no_replace")) { v -> save { it.put("music_no_replace", !v) } }
        Toggle("замены — в папку «_из других мест»", o.optBoolean("music_replace_dir")) { v -> save { it.put("music_replace_dir", v) } }
        Toggle("проверять замену по звуку (Deezer)", o.optBoolean("music_verify", true)) { v -> save { it.put("music_verify", v) } }
        Toggle("тексты песен (LRCLIB)", o.optBoolean("lyrics")) { v -> save { it.put("lyrics", v) } }
        Text(
            "Spotify отдаёт без ключа только первые 100 треков плейлиста. Ключ — developer.spotify.com → Create app.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        KeyField("Spotify Client ID", o.optString("spotify_id")) { v -> save { it.put("spotify_id", v) } }
        KeyField("Spotify Client secret", o.optString("spotify_secret")) { v -> save { it.put("spotify_secret", v) } }

        // ---------- аккаунты ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Accounts(o, ::save)

        // ---------- папка ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("Папка загрузок")
        SortExisting(o.optString("out_dir"))
        TopicsEditor()

        // ---------- оформление ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("Оформление")
        val themes = remember { runCatching { themeList() }.getOrDefault(emptyList()) }
        val options = buildList {
            add(Style.AUTO to "по марке телефона (${Style.brandName()}: ${Style.label(Style.brandStyle())})")
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(Style.YOU to "Material You — цвета из обоев")
            Style.BRANDS.forEach { (k, label) -> add(k to label) }
            themes.forEach { add(Style.THEME_PREFIX + it.key to "тема с ПК: ${it.name}") }
        }
        options.forEach { (key, label) ->
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                RadioButton(selected = style == key, onClick = { onStyle(key) })
                Text(label)
            }
        }

        // ---------- телефон ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("Телефон")
        BatteryRow()

        // ---------- yt-dlp ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("yt-dlp")
        Text("версия: $ytdlp", color = MaterialTheme.colorScheme.onSurfaceVariant)
        FilledTonalButton(enabled = !updating, onClick = {
            updating = true
            scope.launch(Dispatchers.IO) {
                ytdlp = try {
                    updateYtdlp()
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: обновление yt-dlp: ${e.message}")
                    ytdlpVersion() ?: "?"
                }
                updating = false
            }
        }) { Text(if (updating) "обновляю…" else "обновить yt-dlp") }
        Text(
            "YouTube регулярно ломает старые версии yt-dlp — если видео перестали качаться, обновите.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        // ---------- проверка ----------
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("Проверка")
        var checking by remember { mutableStateOf(false) }
        Text(
            "yt-dlp, кодеки и фильтры ffmpeg, форматы YouTube с QuickJS и без — результат в «Журнале».",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        FilledTonalButton(enabled = !checking, onClick = {
            checking = true
            scope.launch(Dispatchers.IO) {
                Vgd.addLog("Проверка: ролик 240p (Me at the zoo) и ролик 4K (Big Buck Bunny)…")
                try {
                    Vgd.core()
                    for (url in listOf("https://www.youtube.com/watch?v=jNQXAC9IVRw", "https://www.youtube.com/watch?v=aqz-KE-bpKQ")) {
                        Vgd.addLog("— $url")
                        io.github.vivegass.vgd.core.diagnose(url).forEach { Vgd.addLog(it) }
                    }
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: проверка: ${e.message}")
                }
                checking = false
            }
        }) { Text(if (checking) "проверяю…" else "проверить") }
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Text(
            "Very Good Downloader для Android ${BuildConfig.VERSION_NAME} · ядро ${io.github.vivegass.vgd.core.version()} · yt-dlp $ytdlp\n${ctx.packageName}",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// ---------- папка: системный выбор ----------

/** content://…/tree/primary:Download/X → /storage/emulated/0/Download/X (с «Доступом ко всем файлам» — обычный путь). */
internal fun treeToPath(uri: Uri): String? {
    val id = try { DocumentsContract.getTreeDocumentId(uri) } catch (_: Exception) { return null }
    val (vol, rel) = id.split(':', limit = 2).let { it[0] to it.getOrElse(1) { "" } }
    val root = if (vol.equals("primary", ignoreCase = true)) Environment.getExternalStorageDirectory().absolutePath else "/storage/$vol"
    return if (rel.isEmpty()) root else "$root/$rel"
}

@Composable
private fun FolderRow(current: String, onChange: (String) -> Unit) {
    var dir by remember(current) { mutableStateOf(current) }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        val p = uri?.let { treeToPath(it) } ?: return@rememberLauncherForActivityResult
        dir = p
        onChange(p)
    }
    OutlinedTextField(
        value = dir,
        onValueChange = { dir = it },
        label = { Text("папка") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
        trailingIcon = {
            TextButton(onClick = { onChange(dir.trim().ifEmpty { Vgd.defaultDownloadDir().absolutePath }) }) { Text("ок") }
        },
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = { pick.launch(null) }) { Text("выбрать…") }
        TextButton(onClick = {
            dir = Vgd.defaultDownloadDir().absolutePath
            onChange(dir)
        }) { Text("по умолчанию") }
    }
}

// ---------- батарея ----------

@SuppressLint("BatteryLife")
@Composable
private fun BatteryRow() {
    val ctx = LocalContext.current
    var ignoring by remember { mutableStateOf(ignoresBattery(ctx)) }
    LifecycleResumeEffect(Unit) {
        ignoring = ignoresBattery(ctx)
        onPauseOrDispose { }
    }
    Text(
        if (ignoring) "Оптимизация батареи для приложения выключена — система не остановит долгую загрузку."
        else "Чтобы система не останавливала долгие загрузки (особенно Xiaomi, Huawei, Samsung), выключите для приложения оптимизацию батареи.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    if (!ignoring) {
        FilledTonalButton(onClick = {
            try {
                ctx.startActivity(Intent(AndroidSettings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + ctx.packageName)))
            } catch (_: Exception) {
                ctx.startActivity(Intent(AndroidSettings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
            }
        }) { Text("не оптимизировать батарею") }
    }
}

internal fun ignoresBattery(ctx: Context): Boolean =
    (ctx.getSystemService(Context.POWER_SERVICE) as PowerManager).isIgnoringBatteryOptimizations(ctx.packageName)

// ---------- аккаунты ----------

@Composable
private fun Accounts(o: JSONObject, save: ((JSONObject) -> Unit) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    Section("Аккаунты")

    // VK: вход на vk.com в окне программы, поиск — скрытый WebView
    var vkLogged by remember { mutableStateOf(VkAgent.loggedIn()) }
    val vkLogin = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { vkLogged = VkAgent.loggedIn() }
    Text("VK: " + if (vkLogged) "вход выполнен" else "нет входа", style = MaterialTheme.typography.titleSmall)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilledTonalButton(onClick = { vkLogin.launch(LoginActivity.intent(ctx, LoginActivity.VK)) }) { Text(if (vkLogged) "открыть vk.com" else "войти в VK") }
    }
    Toggle("искать музыку в VK первым (звук прямо из VK)", o.optBoolean("vk_search")) { v -> save { it.put("vk_search", v) } }

    // Яндекс: вход кодом (ya.ru/device)
    var yaLogged by remember { mutableStateOf(runCatching { yandexLoggedIn() }.getOrDefault(false)) }
    var yaCode by remember { mutableStateOf<String?>(null) }
    Text("Яндекс Музыка: " + if (yaLogged) "вход выполнен" else "нет входа", style = MaterialTheme.typography.titleSmall)
    if (yaCode != null) {
        Notice("Откройте ya.ru/device и введите код: $yaCode (латиницей). Жду подтверждения…", null) {}
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (!yaLogged) {
            FilledTonalButton(enabled = yaCode == null, onClick = {
                scope.launch(Dispatchers.IO) {
                    try {
                        val d = yandexStart()
                        yaCode = d.userCode
                        withContext(Dispatchers.Main) {
                            runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(d.url))) }
                        }
                        val end = System.currentTimeMillis() + d.expiresIn.toLong() * 1000
                        while (System.currentTimeMillis() < end) {
                            delay(d.interval.toLong() * 1000)
                            if (yandexPoll(d.deviceCode)) {
                                yaLogged = true
                                break
                            }
                        }
                    } catch (e: Throwable) {
                        Vgd.addLog("Ошибка: вход в Яндекс: ${e.message}")
                    }
                    yaCode = null
                    if (yaLogged) save { it.put("yandex_acc", true) }
                }
            }) { Text("войти в Яндекс") }
        } else {
            OutlinedButton(onClick = {
                yandexLogout()
                yaLogged = false
                save { it.put("yandex_acc", false) }
            }) { Text("выйти") }
        }
    }
    if (yaLogged) {
        Toggle("звук прямо с Яндекса и поиск замены там", o.optBoolean("yandex_acc")) { v -> save { it.put("yandex_acc", v) } }
    }

    // YouTube: куки для 18+ и закрытых видео (только при повторе — с куки YouTube отдаёт урезанные форматы)
    val cookies = LoginActivity.cookiesFile(ctx)
    var hasCookies by remember { mutableStateOf(cookies.isFile) }
    fun useCookies() = save {
        it.put("cookies", "file")
        it.put("cookies_file", cookies.absolutePath)
    }
    val ytLogin = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        hasCookies = cookies.isFile
        if (hasCookies) useCookies()
    }
    val pickCookies = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        try {
            cookies.parentFile?.mkdirs()
            ctx.contentResolver.openInputStream(uri)?.use { inp -> cookies.outputStream().use { inp.copyTo(it) } }
            hasCookies = true
            useCookies()
            Vgd.addLog("YouTube: cookies.txt загружен")
        } catch (e: Exception) {
            Vgd.addLog("Ошибка: cookies.txt: ${e.message}")
        }
    }
    Text("YouTube (18+, закрытые): " + if (hasCookies) "куки есть" else "без входа", style = MaterialTheme.typography.titleSmall)
    Text(
        "Куки берутся только при повторе, если видео требует входа. Google может не пустить во встроенный браузер — " +
            "тогда выгрузите cookies.txt на ПК (расширение «Get cookies.txt») и выберите файл.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilledTonalButton(onClick = { ytLogin.launch(LoginActivity.intent(ctx, LoginActivity.YOUTUBE)) }) { Text("войти") }
        OutlinedButton(onClick = { pickCookies.launch(arrayOf("text/plain", "*/*")) }) { Text("cookies.txt…") }
        if (hasCookies) {
            TextButton(onClick = {
                cookies.delete()
                hasCookies = false
                save { it.put("cookies", "none") }
            }) { Text("убрать") }
        }
    }
}

// ---------- «Разложить папку» ----------

@Composable
private fun SortExisting(root: String) {
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var plan by remember { mutableStateOf<List<MovePlan>?>(null) }
    Text(
        "Разложить уже скачанные видео (с [id] в имени или в теге) по папкам «Тема / Канал». Сначала — план.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    FilledTonalButton(enabled = !busy, onClick = {
        busy = true
        scope.launch(Dispatchers.IO) {
            plan = try {
                Vgd.core()
                sortPlan(root)
            } catch (e: Throwable) {
                Vgd.addLog("Ошибка: разложить: ${e.message}")
                null
            }
            if (plan?.isEmpty() == true) {
                Vgd.addLog("Разложить: в корне папки нет неразложенных видео, скачанных программой.")
                plan = null
            }
            busy = false
        }
    }) { Text(if (busy) "смотрю…" else "разложить папку…") }
    val p = plan
    if (p != null) {
        AlertDialog(
            onDismissRequest = { plan = null },
            title = { Text("Разложить ${p.size} файл(ов)?") },
            text = {
                Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
                    p.take(200).forEach { m ->
                        Text(File(m.from).name + " → " + m.to.removePrefix(root).trimStart('/'), style = MaterialTheme.typography.bodySmall)
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    plan = null
                    scope.launch(Dispatchers.IO) {
                        val n = sortApply(p)
                        Vgd.addLog("Разложено файлов: $n")
                    }
                }) { Text("разложить") }
            },
            dismissButton = { TextButton(onClick = { plan = null }) { Text("отмена") } },
        )
    }
}

// ---------- категории (topics.json) ----------

@Composable
private fun TopicsEditor() {
    var open by remember { mutableStateOf(false) }
    OutlinedButton(onClick = { open = true }) { Text("категории…") }
    if (!open) return
    var rows by remember { mutableStateOf(runCatching { topics() }.getOrDefault(emptyList()).map { it.name to it.keywords.joinToString(", ") }) }
    AlertDialog(
        onDismissRequest = { open = false },
        title = { Text("Категории") },
        text = {
            Column(Modifier.heightIn(max = 460.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    "Тема видео — по ключевым словам (название ×3, плейлист ×4, теги, ник канала). Выше — важнее.",
                    style = MaterialTheme.typography.bodySmall,
                )
                rows.forEachIndexed { i, (name, kw) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            OutlinedTextField(name, { v -> rows = rows.toMutableList().also { it[i] = v to kw } }, label = { Text("название") }, singleLine = true)
                            OutlinedTextField(kw, { v -> rows = rows.toMutableList().also { it[i] = name to v } }, label = { Text("ключевые слова через запятую") })
                        }
                        IconButton(onClick = { rows = rows.toMutableList().also { it.removeAt(i) } }) { Icon(Icons.Filled.Close, contentDescription = "убрать") }
                    }
                }
                Row {
                    TextButton(onClick = { rows = rows + ("" to "") }) { Text("добавить") }
                    TextButton(onClick = { rows = defaultTopics().map { it.name to it.keywords.joinToString(", ") } }) { Text("как было") }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                try {
                    saveTopics(rows.map { (n, k) -> Topic(n, k.split(',').map { it.trim() }.filter { it.isNotEmpty() }) })
                } catch (e: Throwable) {
                    Vgd.addLog("Ошибка: категории: ${e.message}")
                }
                open = false
            }) { Text("сохранить") }
        },
        dismissButton = { TextButton(onClick = { open = false }) { Text("отмена") } },
    )
}

// ---------- общие элементы ----------

@Composable
internal fun Section(title: String) {
    Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 8.dp))
}

@Composable
internal fun Toggle(label: String, value: Boolean, onChange: (Boolean) -> Unit) {
    var v by remember(value) { mutableStateOf(value) }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(label, modifier = Modifier.weight(1f))
        Spacer(Modifier.width(8.dp))
        Switch(checked = v, onCheckedChange = { v = it; onChange(it) })
    }
}

@Composable
internal fun Chips(options: List<Pair<Int, String>>, value: Int, onChange: (Int) -> Unit) {
    var v by remember(value) { mutableIntStateOf(value) }
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (k, label) ->
            FilterChip(
                selected = v == k,
                onClick = { v = k; onChange(k) },
                label = { Text(label) },
                colors = FilterChipDefaults.filterChipColors(
                    selectedContainerColor = MaterialTheme.colorScheme.primaryContainer,
                    selectedLabelColor = MaterialTheme.colorScheme.onPrimaryContainer,
                ),
            )
        }
    }
}

@Composable
private fun KeyField(label: String, value: String, onSave: (String) -> Unit) {
    var v by remember(value) { mutableStateOf(value) }
    OutlinedTextField(
        value = v,
        onValueChange = { v = it },
        label = { Text(label) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
        trailingIcon = { if (v != value) TextButton(onClick = { onSave(v.trim()) }) { Text("ок") } },
    )
}
