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
import androidx.compose.material.icons.outlined.Subscriptions
import androidx.compose.material.icons.filled.Share
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
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.NavigationBarItemDefaults
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
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
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

private enum class Page(val label: Int) { Download(R.string.tab_download), Subs(R.string.tab_subs), Log(R.string.tab_log), Settings(R.string.tab_settings) }

private fun pageIcon(p: Page) = when (p) {
    Page.Download -> Icons.Outlined.Download
    Page.Subs -> Icons.Outlined.Subscriptions
    Page.Log -> Icons.Outlined.Description
    Page.Settings -> Icons.Outlined.Settings
}

@Composable
fun App(style: String, onStyle: (String) -> Unit) {
    var page by rememberSaveable { mutableStateOf(Page.Download) }
    val st = LocalVgdStyle.current
    val scroll = TopAppBarDefaults.pinnedScrollBehavior()
    val jobs by Vgd.jobs.collectAsState()
    val active = jobs.count { it.state == JobState.QUEUED || it.state == JobState.RUNNING }
    fun label(p: Page, ctx: Context) = ctx.getString(p.label) + if (p == Page.Download && active > 0) " · $active" else ""
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        containerColor = screenBackground(),
        topBar = {
            val title = stringResource(page.label)
            when (st.header) {
                // Один и тот же заголовок на всех экранах (крупный раскрывался по-разному в зависимости от прокрутки).
                Header.SMALL -> TopAppBar(title = { Text(title) }, scrollBehavior = scroll)
                // iOS / HyperOS / HarmonyOS: крупный заголовок слева
                Header.LARGE -> Text(
                    title,
                    style = MaterialTheme.typography.headlineLarge.copy(fontWeight = FontWeight.Bold),
                    modifier = Modifier.statusBarsPadding().padding(start = 20.dp, top = 12.dp, bottom = 8.dp),
                )
                // One UI: заголовок в верхней трети, содержимое ниже — под большой палец
                Header.HUGE -> Box(
                    Modifier.fillMaxWidth().statusBarsPadding().height(150.dp).padding(start = 24.dp, bottom = 20.dp),
                    contentAlignment = Alignment.BottomStart,
                ) {
                    Text(title, style = MaterialTheme.typography.displaySmall)
                }
            }
        },
        bottomBar = {
            val ctx = LocalContext.current
            when (st.nav) {
                Nav.BAR -> NavigationBar(
                    containerColor = if (st.glass) MaterialTheme.colorScheme.surfaceContainer.copy(alpha = 0.85f) else MaterialTheme.colorScheme.surfaceContainer,
                ) {
                    Page.entries.forEach { p ->
                        NavigationBarItem(
                            selected = page == p,
                            onClick = { page = p },
                            icon = { Icon(pageIcon(p), contentDescription = null) },
                            label = { Text(label(p, ctx)) },
                            colors = NavigationBarItemDefaults.colors(
                                indicatorColor = MaterialTheme.colorScheme.primaryContainer,
                                selectedIconColor = MaterialTheme.colorScheme.onPrimaryContainer,
                            ),
                        )
                    }
                }
                // плавающая панель-«таблетка» (One UI, HyperOS, iOS)
                Nav.PILL -> Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 10.dp), contentAlignment = Alignment.Center) {
                    Surface(
                        shape = RoundedCornerShape(50),
                        color = MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = if (st.glass) 0.88f else 1f),
                        border = BorderStroke(0.5.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f)),
                        shadowElevation = 6.dp,
                    ) {
                        Row(Modifier.padding(horizontal = 6.dp, vertical = 6.dp)) {
                            Page.entries.forEach { p ->
                                val sel = page == p
                                Surface(
                                    onClick = { page = p },
                                    shape = RoundedCornerShape(50),
                                    color = if (sel) MaterialTheme.colorScheme.primaryContainer else Color.Transparent,
                                ) {
                                    Column(Modifier.padding(horizontal = 14.dp, vertical = 6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                                        Icon(
                                            pageIcon(p),
                                            contentDescription = null,
                                            tint = if (sel) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                        Text(
                                            label(p, ctx),
                                            style = MaterialTheme.typography.labelSmall,
                                            color = if (sel) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    ) { pad ->
        when (page) {
            Page.Download -> DownloadPage(pad)
            Page.Subs -> SubsPage(pad)
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
    var showOpts by rememberSaveable { mutableStateOf(false) }
    var from by rememberSaveable { mutableStateOf("") }
    var to by rememberSaveable { mutableStateOf("") }
    var split by rememberSaveable { mutableStateOf(false) }
    var preset by rememberSaveable { mutableStateOf("") }
    val presets = remember { runCatching { org.json.JSONObject(io.github.vivegass.vgd.core.settingsJson()).optJSONArray("presets") }.getOrNull() }
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
        // Параметры ссылки — как JobOpts на ПК (фрагмент «с … до …», только звук, микс → треки, пресет).
        val opts = org.json.JSONObject()
            .put("audio_only", audio)
            .put("from", from.trim())
            .put("to", to.trim())
            .put("split_tracks", split)
            .put("preset", preset)
        urls.forEach { Vgd.addWith(ctx, it, opts.toString()) }
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
                Text("только звук")
                TextButton(onClick = { showOpts = !showOpts }) { Text(if (showOpts) "меньше" else "ещё…") }
                Spacer(Modifier.weight(1f))
                Button(onClick = { start() }, enabled = text.isNotBlank()) {
                    Icon(Icons.Filled.Download, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text("скачать")
                }
            }
        }
        if (showOpts) {
            item {
                VgdCard {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text("фрагмент (пусто — целиком)", style = MaterialTheme.typography.labelLarge)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedTextField(from, { from = it }, label = { Text("с 1:30") }, singleLine = true, modifier = Modifier.weight(1f))
                            OutlinedTextField(to, { to = it }, label = { Text("до 2:45") }, singleLine = true, modifier = Modifier.weight(1f))
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(checked = split, onCheckedChange = { split = it })
                            Text("микс / альбом одним видео → отдельные треки")
                        }
                        if (presets != null && presets.length() > 0) {
                            Text("пресет", style = MaterialTheme.typography.labelLarge)
                            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                (listOf("") + (0 until presets.length()).map { presets.getJSONObject(it).optString("name") }).forEach { name ->
                                    FilterChip(
                                        selected = preset == name,
                                        onClick = { preset = name },
                                        label = { Text(name.ifEmpty { "обычные настройки" }) },
                                        colors = FilterChipDefaults.filterChipColors(
                                            selectedContainerColor = MaterialTheme.colorScheme.primaryContainer,
                                            selectedLabelColor = MaterialTheme.colorScheme.onPrimaryContainer,
                                        ),
                                    )
                                }
                            }
                        }
                    }
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
                if (jobs.any { it.state == JobState.PAUSED }) {
                    TextButton(onClick = {
                        io.github.vivegass.vgd.DownloadService.start(ctx)
                        act { it.resumeAll() }
                    }) { Text("продолжить всё") }
                } else if (jobs.any { it.state == JobState.QUEUED || it.state == JobState.RUNNING }) {
                    TextButton(onClick = { act { it.pauseAll() } }) { Text("пауза") }
                }
                if (jobs.any { it.state != JobState.QUEUED && it.state != JobState.RUNNING && it.state != JobState.PAUSED }) {
                    TextButton(onClick = { act { it.clearDone() } }) { Text("убрать готовые") }
                }
            }
        }
        if (jobs.isEmpty()) {
            item {
                ShareHint()
            }
            item {
                Text(
                    "пусто — вставь ссылку выше или поделись ссылкой из YouTube",
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
internal fun act(f: (io.github.vivegass.vgd.core.Core) -> Unit) {
    Thread {
        try {
            f(Vgd.core())
        } catch (e: Throwable) {
            Vgd.addLog("Ошибка: $e")
        }
    }.start()
}

@Composable
internal fun Notice(text: String, button: String?, onClick: () -> Unit) {
    VgdCard(MaterialTheme.colorScheme.secondaryContainer) {
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
    VgdCard(modifier = Modifier.fillMaxWidth()) {
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
        val ctx = LocalContext.current
        OutlinedTextField(
            value = find,
            onValueChange = { find = it },
            label = { Text("поиск в журнале") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
            trailingIcon = {
                // отправить журнал (например, в Telegram — разобраться, что не скачалось)
                IconButton(onClick = {
                    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, shown.takeLast(500).joinToString("\n"))
                    ctx.startActivity(Intent.createChooser(send, "журнал Very Good Downloader"))
                }) { Icon(Icons.Filled.Share, contentDescription = "отправить журнал") }
            },
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

/**
 * Как сделать «Поделиться» в одно нажатие. Свой ряд приложений в панели YouTube составляет сам YouTube
 * (туда сторонние приложения не попадают); в системной панели («Ещё») приложение можно закрепить сверху.
 */
@Composable
private fun ShareHint() {
    VgdCard(MaterialTheme.colorScheme.primaryContainer) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Скачивать прямо из YouTube", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onPrimaryContainer)
            Text(
                "«Поделиться» → «Ещё» → «VGD: скачать» (или «VGD: звук»). Чтобы это было сверху: в той же панели " +
                    "долго нажмите «VGD: скачать» → «Закрепить». В короткий ряд самого YouTube (Telegram, Discord…) " +
                    "сторонние приложения не попадают — его составляет YouTube.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
            )
        }
    }
}
