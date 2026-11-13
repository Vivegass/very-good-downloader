@file:OptIn(ExperimentalMaterial3Api::class)

package io.github.vivegass.vgd.ui

import android.Manifest
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.github.vivegass.vgd.R
import io.github.vivegass.vgd.Vgd
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState
import io.github.vivegass.vgd.core.saveSettingsJson
import io.github.vivegass.vgd.core.settingsJson
import io.github.vivegass.vgd.core.themeList
import io.github.vivegass.vgd.core.updateYtdlp
import io.github.vivegass.vgd.core.ytdlpVersion
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

private enum class Page(val label: Int) { Download(R.string.tab_download), Log(R.string.tab_log), Settings(R.string.tab_settings) }

@Composable
fun App(style: String, onStyle: (String) -> Unit) {
    var page by rememberSaveable { mutableStateOf(Page.Download) }
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            // Крупный заголовок, сворачивается при прокрутке (M3).
            LargeTopAppBar(title = { Text(stringResource(page.label)) }, scrollBehavior = scroll)
        },
        bottomBar = {
            NavigationBar {
                val jobs by Vgd.jobs.collectAsState()
                val active = jobs.count { it.state == JobState.QUEUED || it.state == JobState.RUNNING }
                Page.entries.forEach { p ->
                    NavigationBarItem(
                        selected = page == p,
                        onClick = { page = p },
                        icon = {
                            Icon(
                                when (p) {
                                    Page.Download -> Icons.Outlined.Download
                                    Page.Log -> Icons.Outlined.Description
                                    Page.Settings -> Icons.Outlined.Settings
                                },
                                contentDescription = null,
                            )
                        },
                        label = { Text(stringResource(p.label) + if (p == Page.Download && active > 0) " · $active" else "") },
                    )
                }
            }
        },
    ) { pad ->
        when (page) {
            Page.Download -> DownloadPage(pad)
            Page.Log -> LogPage(pad)
            Page.Settings -> SettingsPage(pad, style, onStyle)
        }
    }
}

// ---------- «Скачать» ----------

private fun hasAllFiles(ctx: Context): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
    else ContextCompat.checkSelfPermission(ctx, Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED

@Composable
private fun DownloadPage(pad: PaddingValues) {
    val ctx = LocalContext.current
    val jobs by Vgd.jobs.collectAsState()
    val ready by Vgd.ready.collectAsState()
    var text by rememberSaveable { mutableStateOf("") }
    var audio by rememberSaveable { mutableStateOf(false) }
    var allFiles by remember { mutableStateOf(hasAllFiles(ctx)) }
    LifecycleResumeEffect(Unit) {
        allFiles = hasAllFiles(ctx)
        onPauseOrDispose { }
    }
    val notifPerm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    val storagePerm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allFiles = hasAllFiles(ctx) }
    LaunchedEffect(Unit) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) notifPerm.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    fun start() {
        val lines = text.lines().map { it.trim() }.filter { it.isNotEmpty() }
        val urls = lines.mapNotNull { io.github.vivegass.vgd.core.extractUrl(it) }
        urls.forEach { Vgd.add(ctx, it, audio) }
        if (urls.isNotEmpty()) text = ""
    }

    LazyColumn(
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = pad.calculateTopPadding() + 4.dp, bottom = pad.calculateBottomPadding() + 16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("ссылки — по одной в строке") },
                modifier = Modifier.fillMaxWidth(),
                minLines = 2,
                maxLines = 6,
                trailingIcon = {
                    IconButton(onClick = {
                        val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        val t = cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(ctx)?.toString().orEmpty()
                        if (t.isNotBlank()) text = if (text.isBlank()) t.trim() else text.trimEnd() + "\n" + t.trim()
                    }) { Icon(Icons.Filled.ContentPaste, contentDescription = "вставить") }
                },
            )
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = audio, onCheckedChange = { audio = it })
                Text("только звук", modifier = Modifier.weight(1f))
                Button(onClick = { start() }, enabled = text.isNotBlank()) {
                    Icon(Icons.Filled.Download, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text("скачать")
                }
            }
        }
        if (!allFiles) {
            item {
                Notice(
                    "Нужен «Доступ ко всем файлам»: видео сохраняются в Download/Very Good Downloader и раскладываются по папкам.",
                    "разрешить",
                ) {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        val i = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + ctx.packageName))
                        try {
                            ctx.startActivity(i)
                        } catch (_: Exception) {
                            ctx.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
                        }
                    } else {
                        storagePerm.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
                    }
                }
            }
        }
        when {
            ready == null -> item { Text("готовлю yt-dlp и ffmpeg… (первый запуск — несколько секунд)", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            ready!!.isNotEmpty() -> item { Notice("Не удалось подготовить yt-dlp: $ready", null) {} }
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("очередь", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                if (jobs.any { it.state != JobState.QUEUED && it.state != JobState.RUNNING && it.state != JobState.PAUSED }) {
                    TextButton(onClick = { act { it.clearDone() } }) { Text("убрать готовые") }
                }
            }
        }
        if (jobs.isEmpty()) {
            item {
                Text(
                    "пусто — вставь ссылку выше или нажми «Поделиться» → Very Good Downloader в YouTube",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
        items(jobs, key = { it.id.toLong() }) { j ->
            JobCard(j, first = j == jobs.first(), last = j == jobs.last())
        }
    }
}

/** Действие с очередью — не в главной нити (ядро могло ещё распаковываться). */
private fun act(f: (io.github.vivegass.vgd.core.Core) -> Unit) {
    Thread {
        try {
            f(Vgd.core())
        } catch (e: Throwable) {
            Vgd.addLog("Ошибка: $e")
        }
    }.start()
}

@Composable
private fun Notice(text: String, button: String?, onClick: () -> Unit) {
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer)) {
        Column(Modifier.padding(16.dp)) {
            Text(text, color = MaterialTheme.colorScheme.onSecondaryContainer)
            if (button != null) {
                Spacer(Modifier.height(8.dp))
                FilledTonalButton(onClick = onClick) { Text(button) }
            }
        }
    }
}

private fun stageName(s: String): String = when (s) {
    "queued" -> "в очереди"
    "info" -> "получаю сведения"
    "video" -> "видео"
    "audio" -> "звук"
    "merge" -> "склеиваю"
    "edit" -> "копия для монтажа"
    "subs" -> "субтитры"
    "wait" -> "жду"
    "done" -> "готово"
    "error" -> "ошибка"
    "cancel" -> "отменено"
    "paused" -> "на паузе"
    else -> s
}

@Composable
private fun JobCard(j: JobInfo, first: Boolean, last: Boolean) {
    val ctx = LocalContext.current
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        shape = MaterialTheme.shapes.large,
    ) {
        Column(Modifier.padding(start = 16.dp, end = 8.dp, top = 12.dp, bottom = 8.dp)) {
            Text(j.title.ifEmpty { j.url }, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            val line = listOf(
                stageName(j.stage),
                j.item,
                j.message,
                j.speed,
                j.eta.takeIf { it.isNotEmpty() }?.let { "осталось $it" } ?: "",
            ).filter { it.isNotEmpty() }.distinct().joinToString(" · ")
            Text(line, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (j.state == JobState.RUNNING) {
                Spacer(Modifier.height(6.dp))
                if (j.percent > 0f) {
                    LinearProgressIndicator(progress = { j.percent / 100f }, modifier = Modifier.fillMaxWidth().padding(end = 8.dp))
                } else {
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth().padding(end = 8.dp))
                }
            }
            if (j.error.isNotEmpty()) {
                Text(j.error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
            }
            if (j.notice.isNotEmpty()) {
                Text(j.notice, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.tertiary)
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Spacer(Modifier.weight(1f))
                when (j.state) {
                    JobState.QUEUED, JobState.RUNNING -> IconButton(onClick = { act { it.pause(j.id) } }) {
                        Icon(Icons.Filled.Pause, contentDescription = "пауза")
                    }
                    JobState.PAUSED -> IconButton(onClick = {
                        io.github.vivegass.vgd.DownloadService.start(ctx)
                        act { it.resume(j.id) }
                    }) { Icon(Icons.Filled.PlayArrow, contentDescription = "продолжить") }
                    JobState.ERROR, JobState.CANCELLED -> IconButton(onClick = {
                        io.github.vivegass.vgd.DownloadService.start(ctx)
                        act { it.retry(j.id) }
                    }) { Icon(Icons.Filled.Refresh, contentDescription = "повторить") }
                    JobState.DONE -> {}
                }
                IconButton(onClick = { act { it.moveJob(j.id, -1) } }, enabled = !first) {
                    Icon(Icons.Filled.ArrowUpward, contentDescription = "выше")
                }
                IconButton(onClick = { act { it.moveJob(j.id, 1) } }, enabled = !last) {
                    Icon(Icons.Filled.ArrowDownward, contentDescription = "ниже")
                }
                IconButton(onClick = {
                    act { c -> if (j.state == JobState.QUEUED || j.state == JobState.RUNNING || j.state == JobState.PAUSED) c.cancel(j.id) else c.remove(j.id) }
                }) { Icon(Icons.Filled.Close, contentDescription = "отменить") }
            }
        }
    }
}

// ---------- «Журнал» ----------

@Composable
private fun LogPage(pad: PaddingValues) {
    val log by Vgd.log.collectAsState()
    var find by rememberSaveable { mutableStateOf("") }
    val shown = remember(log, find) { if (find.isBlank()) log else log.filter { it.contains(find, ignoreCase = true) } }
    val state = rememberLazyListState()
    LaunchedEffect(shown.size) { if (shown.isNotEmpty()) state.scrollToItem(shown.size - 1) }
    Column(Modifier.fillMaxSize().padding(top = pad.calculateTopPadding(), bottom = pad.calculateBottomPadding())) {
        OutlinedTextField(
            value = find,
            onValueChange = { find = it },
            label = { Text("поиск в журнале") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        )
        SelectionContainer {
            LazyColumn(state = state, contentPadding = PaddingValues(16.dp), modifier = Modifier.fillMaxSize()) {
                items(shown) { line ->
                    val err = line.startsWith("Ошибка")
                    Text(
                        line,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        color = if (err) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(vertical = 1.dp),
                    )
                }
            }
        }
    }
}

// ---------- «Настройки» ----------

@Composable
private fun SettingsPage(pad: PaddingValues, style: String, onStyle: (String) -> Unit) {
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
        Section("Загрузки")
        var dir by remember { mutableStateOf(o.optString("out_dir")) }
        OutlinedTextField(
            value = dir,
            onValueChange = { dir = it },
            label = { Text("папка") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            trailingIcon = {
                TextButton(onClick = { save { it.put("out_dir", dir.trim().ifEmpty { Vgd.defaultDownloadDir().absolutePath }) } }) { Text("ок") }
            },
        )
        TextButton(onClick = {
            dir = Vgd.defaultDownloadDir().absolutePath
            save { it.put("out_dir", dir) }
        }) { Text("по умолчанию: Download/Very Good Downloader") }

        Text("разрешение", style = MaterialTheme.typography.labelLarge)
        Chips(listOf(0 to "макс", 2160 to "2160p", 1440 to "1440p", 1080 to "1080p", 720 to "720p", 480 to "480p"), o.optInt("max_height")) { v ->
            save { it.put("max_height", v) }
        }
        Text("кадров в секунду", style = MaterialTheme.typography.labelLarge)
        Chips(listOf(0 to "макс", 60 to "60", 30 to "30", 24 to "24"), o.optInt("max_fps")) { v -> save { it.put("max_fps", v) } }

        Toggle("раскладывать по папкам «Тема / Канал»", o.optBoolean("sort")) { v -> save { it.put("sort", v) } }
        Toggle("плейлисты и каналы целиком", o.optBoolean("playlist", true)) { v -> save { it.put("playlist", v) } }
        Toggle("субтитры (${o.optString("sub_langs", "ru,en")})", o.optBoolean("subs")) { v -> save { it.put("subs", v) } }
        Toggle("обложка .jpg рядом с видео", o.optBoolean("thumbnail", true)) { v -> save { it.put("thumbnail", v) } }

        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Section("Оформление")
        val themes = remember { runCatching { themeList() }.getOrDefault(emptyList()) }
        val options = buildList {
            add(Style.AUTO to "по марке телефона (${Style.brandName()}: Material You)")
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(Style.YOU to "Material You — цвета из обоев")
            themes.forEach { add(Style.THEME_PREFIX + it.key to "тема с ПК: ${it.name}") }
        }
        options.forEach { (key, label) ->
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                androidx.compose.material3.RadioButton(selected = style == key, onClick = { onStyle(key) })
                Text(label)
            }
        }

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
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Text(
            "Very Good Downloader ${io.github.vivegass.vgd.core.version()} · ${ctx.packageName}",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Section(title: String) {
    Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 8.dp))
}

@Composable
private fun Toggle(label: String, value: Boolean, onChange: (Boolean) -> Unit) {
    var v by remember(value) { mutableStateOf(value) }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = v, onCheckedChange = { v = it; onChange(it) })
    }
}

@Composable
private fun Chips(options: List<Pair<Int, String>>, value: Int, onChange: (Int) -> Unit) {
    var v by remember(value) { mutableIntStateOf(value) }
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (k, label) ->
            FilterChip(selected = v == k, onClick = { v = k; onChange(k) }, label = { Text(label) })
        }
    }
}
