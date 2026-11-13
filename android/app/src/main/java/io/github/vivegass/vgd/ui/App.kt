@file:OptIn(ExperimentalMaterial3Api::class)

package io.github.vivegass.vgd.ui

import android.Manifest
import android.content.ClipData
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
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Subscriptions
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Subscriptions
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.ContentPaste
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.IosShare
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.github.vivegass.vgd.DownloadService
import io.github.vivegass.vgd.Vgd
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState
import kotlinx.coroutines.launch
import org.json.JSONObject

// ---------- каркас ----------

private enum class Page { Download, Subs, Log, Settings }

@Composable
fun App(style: String, colors: String, onStyle: (String) -> Unit, onColors: (String) -> Unit) {
    var page by rememberSaveable { mutableIntStateOf(0) }
    val jobs by Vgd.jobs.collectAsState()
    val active = jobs.count { it.state == JobState.QUEUED || it.state == JobState.RUNNING }
    val nav: @Composable () -> Unit = { MainNav(page, active) { page = it } }
    key(page) {
        when (Page.entries[page]) {
            Page.Download -> DownloadScreen(nav)
            Page.Subs -> SubsScreen(nav)
            Page.Log -> LogScreen(nav)
            Page.Settings -> SettingsScreen(nav, style, colors, onStyle, onColors)
        }
    }
}

@Composable
fun MainNav(page: Int, active: Int, onSelect: (Int) -> Unit) {
    VgdNavBar(
        listOf(
            NavItem("Скачать", Icons.Outlined.Download, Icons.Filled.Download, active),
            NavItem("Подписки", Icons.Outlined.Subscriptions, Icons.Filled.Subscriptions),
            NavItem("Журнал", Icons.Outlined.Description, Icons.Filled.Description),
            NavItem("Настройки", Icons.Outlined.Settings, Icons.Filled.Settings),
        ),
        page,
        onSelect,
    )
}

/** Отступ карточек от краёв (у HyperOS списки во всю ширину, а карточки — с полями). */
@Composable
internal fun cardPad(): Dp = LocalVgdStyle.current.hPad.coerceAtLeast(12.dp)

// ---------- действия с очередью ----------

/** Что экран «Скачать» делает с очередью (на телефоне — ядро; в скриншот-тестах — ничего). */
interface QueueActions {
    fun add(urls: List<String>, optsJson: String)
    fun pause(id: ULong)
    fun resume(id: ULong)
    fun retry(id: ULong)
    /** отменить идущую / ждущую и убрать карточку */
    fun remove(id: ULong, active: Boolean)
    fun reorder(ids: List<ULong>)
    fun pauseAll()
    fun resumeAll()
    fun clearDone()
}

class CoreActions(private val ctx: Context) : QueueActions {
    override fun add(urls: List<String>, optsJson: String) = urls.forEach { Vgd.addWith(ctx, it, optsJson) }
    override fun pause(id: ULong) = act { it.pause(id) }
    override fun resume(id: ULong) {
        DownloadService.start(ctx)
        act { it.resume(id) }
    }
    override fun retry(id: ULong) {
        DownloadService.start(ctx)
        act { it.retry(id) }
    }
    override fun remove(id: ULong, active: Boolean) = act { c ->
        if (active) c.cancel(id)
        c.remove(id)
    }
    override fun reorder(ids: List<ULong>) = act { it.reorder(ids) }
    override fun pauseAll() = act { it.pauseAll() }
    override fun resumeAll() {
        DownloadService.start(ctx)
        act { it.resumeAll() }
    }
    override fun clearDone() = act { it.clearDone() }
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

// ---------- «Скачать» ----------

private fun hasAllFiles(ctx: Context): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
    else ContextCompat.checkSelfPermission(ctx, Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED

@Composable
private fun DownloadScreen(nav: @Composable () -> Unit) {
    val ctx = LocalContext.current
    val jobs by Vgd.jobs.collectAsState()
    val ready by Vgd.ready.collectAsState()
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
    val presets = remember {
        runCatching {
            val a = JSONObject(io.github.vivegass.vgd.core.settingsJson()).optJSONArray("presets")
            (0 until (a?.length() ?: 0)).map { a!!.getJSONObject(it).optString("name") }.filter { it.isNotEmpty() }
        }.getOrDefault(emptyList())
    }
    val actions = remember { CoreActions(ctx.applicationContext) }
    DownloadContent(
        jobs = jobs,
        ready = ready,
        allFiles = allFiles,
        presets = presets,
        actions = actions,
        nav = nav,
        onAllFiles = {
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
        },
    )
}

/** Перетаскивание карточек очереди: долгое нажатие → тянуть; соседние карточки расступаются. */
private class DragDrop(val list: LazyListState, val onMove: (Long, Long) -> Unit) {
    var key by mutableStateOf<Long?>(null)
    var offset by mutableFloatStateOf(0f)

    fun start(y: Float): Boolean {
        val it = list.layoutInfo.visibleItemsInfo.firstOrNull { i -> i.key is Long && y.toInt() in i.offset..(i.offset + i.size) }
        key = it?.key as Long?
        offset = 0f
        return key != null
    }

    fun drag(dy: Float): Boolean {
        val k = key ?: return false
        offset += dy
        val items = list.layoutInfo.visibleItemsInfo
        val cur = items.firstOrNull { it.key == k } ?: return false
        val mid = cur.offset + offset + cur.size / 2f
        val target = items.firstOrNull { it.key is Long && it.key != k && mid.toInt() in it.offset..(it.offset + it.size) } ?: return false
        onMove(k, target.key as Long)
        offset += cur.offset - target.offset
        return true
    }

    fun end() {
        key = null
        offset = 0f
    }
}

@Composable
fun DownloadContent(
    jobs: List<JobInfo>,
    ready: String?,
    allFiles: Boolean,
    presets: List<String>,
    actions: QueueActions,
    nav: @Composable () -> Unit,
    onAllFiles: () -> Unit,
    initialText: String = "",
    initialAudio: Boolean = false,
) {
    val ctx = LocalContext.current
    val st = LocalVgdStyle.current
    val pad = cardPad()
    var text by rememberSaveable { mutableStateOf(initialText) }
    var audio by rememberSaveable { mutableStateOf(initialAudio) }
    var showOpts by rememberSaveable { mutableStateOf(false) }
    var from by rememberSaveable { mutableStateOf("") }
    var to by rememberSaveable { mutableStateOf("") }
    var split by rememberSaveable { mutableStateOf(false) }
    var preset by rememberSaveable { mutableStateOf("") }
    var presetSheet by remember { mutableStateOf(false) }
    var shareHint by remember { mutableStateOf(false) }
    val snack = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    var hidden by remember { mutableStateOf(setOf<ULong>()) }
    var order by remember { mutableStateOf<List<ULong>?>(null) }
    val haptic = LocalHapticFeedback.current
    val listState = rememberLazyListState()
    val dd = remember(listState) {
        DragDrop(listState) { from, to ->
            val cur = (order ?: jobs.map { it.id }).toMutableList()
            val a = cur.indexOfFirst { it.toLong() == from }
            val b = cur.indexOfFirst { it.toLong() == to }
            if (a >= 0 && b >= 0) {
                cur.add(b, cur.removeAt(a))
                order = cur
            }
        }
    }
    // Ядро прислало новый порядок — локальный больше не нужен.
    LaunchedEffect(jobs) { if (dd.key == null) order = null }
    val shown = (order?.mapNotNull { id -> jobs.firstOrNull { it.id == id } } ?: jobs).filter { it.id !in hidden }
    val active = jobs.count { it.state == JobState.QUEUED || it.state == JobState.RUNNING }

    fun start() {
        val urls = text.lines().map { it.trim() }.filter { it.isNotEmpty() }.mapNotNull { io.github.vivegass.vgd.core.extractUrl(it) }
        val opts = JSONObject()
            .put("audio_only", audio)
            .put("from", from.trim())
            .put("to", to.trim())
            .put("split_tracks", split)
            .put("preset", preset)
        actions.add(urls, opts.toString())
        if (urls.isNotEmpty()) text = ""
    }

    fun remove(j: JobInfo) {
        hidden = hidden + j.id
        scope.launch {
            val r = snack.showSnackbar("Загрузка убрана из очереди", actionLabel = "Отменить", duration = SnackbarDuration.Short)
            if (r == SnackbarResult.ActionPerformed) {
                hidden = hidden - j.id
            } else {
                actions.remove(j.id, j.state == JobState.QUEUED || j.state == JobState.RUNNING || j.state == JobState.PAUSED)
            }
        }
    }

    VgdScaffold(
        title = "Скачать",
        subtitle = if (active > 0) "Загрузок: $active" else "Видео и музыка по ссылке",
        bottomBar = nav,
        snackbar = snack,
    ) { inner ->
        LazyColumn(
            state = listState,
            contentPadding = PaddingValues(top = inner.calculateTopPadding() + 4.dp, bottom = inner.calculateBottomPadding() + 24.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier.fillMaxSize().pointerInput(dd) {
                detectDragGesturesAfterLongPress(
                    onDragStart = { if (dd.start(it.y)) haptic.performHapticFeedback(HapticFeedbackType.LongPress) },
                    onDrag = { change, amount ->
                        if (dd.key != null) {
                            change.consume()
                            if (dd.drag(amount.y)) haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        }
                    },
                    onDragEnd = {
                        order?.let { actions.reorder(it) }
                        dd.end()
                    },
                    onDragCancel = { dd.end() },
                )
            },
        ) {
            item(key = "input") {
                KitCard(Modifier.fillMaxWidth().padding(horizontal = pad)) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        KitTextField(
                            value = text,
                            onChange = { text = it },
                            placeholder = "Вставьте ссылку (можно несколько, по одной в строке)",
                            minLines = 2,
                            trailing = {
                                IconButton(onClick = {
                                    val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    val t = cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(ctx)?.toString().orEmpty()
                                    if (t.isNotBlank()) text = if (text.isBlank()) t.trim() else text.trimEnd() + "\n" + t.trim()
                                }) { Icon(Icons.Rounded.ContentPaste, contentDescription = "Вставить") }
                            },
                        )
                        KitSegmented(listOf("Видео", "Только звук"), if (audio) 1 else 0, { audio = it == 1 })
                        if (showOpts) {
                            Text("Фрагмент — пусто, чтобы целиком", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                KitTextField(from, { from = it }, "С 1:30", Modifier.weight(1f), singleLine = true)
                                KitTextField(to, { to = it }, "До 2:45", Modifier.weight(1f), singleLine = true)
                            }
                            KitRow(
                                "Микс или альбом → отдельные треки",
                                subtitle = "По главам или таймкодам в описании",
                                modifier = Modifier.clip(RoundedCornerShape(12.dp)),
                                onClick = { split = !split },
                            ) { KitSwitch(split) }
                            if (presets.isNotEmpty()) {
                                KitRow("Пресет", value = preset.ifEmpty { "Обычные настройки" }, nav = true, onClick = { presetSheet = true })
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            KitButton(if (showOpts) "Скрыть" else "Параметры", { showOpts = !showOpts }, kind = Btn.TEXT, icon = Icons.Rounded.Tune)
                            Spacer(Modifier.weight(1f))
                            KitButton("Скачать", { start() }, icon = Icons.Rounded.Download, enabled = text.isNotBlank())
                        }
                    }
                }
            }
            if (!allFiles) {
                item(key = "files") {
                    Notice("Нужен «Доступ ко всем файлам»: видео сохраняются в Download/Very Good Downloader и раскладываются по папкам.", "Разрешить", onAllFiles)
                }
            }
            when {
                ready == null -> item(key = "ready") {
                    Row(Modifier.padding(horizontal = pad + 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        KitProgress(null, Modifier.width(48.dp))
                        Spacer(Modifier.width(12.dp))
                        Text("Готовлю yt-dlp и ffmpeg — первый запуск занимает несколько секунд", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                ready.isNotEmpty() -> item(key = "ready") { Notice("Не удалось подготовить yt-dlp: $ready", null) {} }
            }
            if (jobs.isNotEmpty()) {
                item(key = "head") {
                    Box(Modifier.padding(horizontal = st.hPad.coerceAtLeast(4.dp) - 4.dp)) {
                        SectionHeader("Очередь") {
                            if (jobs.any { it.state == JobState.PAUSED }) {
                                KitButton("Продолжить всё", { actions.resumeAll() }, kind = Btn.TEXT)
                            } else if (active > 0) {
                                KitButton("Пауза", { actions.pauseAll() }, kind = Btn.TEXT)
                            }
                            if (jobs.any { !it.isActive() }) {
                                KitButton("Убрать готовые", { actions.clearDone() }, kind = Btn.TEXT)
                            }
                        }
                    }
                }
            } else {
                item(key = "empty") {
                    EmptyState(
                        Icons.Rounded.Download,
                        "Здесь появятся загрузки",
                        "Вставьте ссылку выше или в YouTube нажмите «Поделиться» → «VGD: скачать».",
                    ) { KitButton("Как скачивать в одно касание", { shareHint = true }, kind = Btn.TONAL, icon = Icons.Rounded.IosShare) }
                }
            }
            items(shown, key = { it.id.toLong() }) { j ->
                val lifted = dd.key == j.id.toLong()
                Box(
                    Modifier.zIndex(if (lifted) 1f else 0f)
                        .graphicsLayer {
                            translationY = if (lifted) dd.offset else 0f
                            scaleX = if (lifted) 1.02f else 1f
                            scaleY = if (lifted) 1.02f else 1f
                        }
                        .then(if (lifted) Modifier else Modifier.animateItem()),
                ) {
                    SwipeToRemove(onRemove = { remove(j) }) {
                        JobCard(j, actions, lifted, onRemove = { remove(j) })
                    }
                }
            }
            if (jobs.size > 1) {
                item(key = "hint") {
                    Text(
                        "Порядок — перетащите карточку, удерживая её. Убрать — смахните влево.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = pad + 8.dp),
                    )
                }
            }
        }
    }
    if (presetSheet) {
        ChoiceSheet(
            "Пресет",
            listOf(Choice("", "Обычные настройки")) + presets.map { Choice(it, it) },
            preset,
            onSelect = { preset = it; presetSheet = false },
            onDismiss = { presetSheet = false },
        )
    }
    if (shareHint) ShareHintDialog { shareHint = false }
}

private fun JobInfo.isActive() = state == JobState.QUEUED || state == JobState.RUNNING || state == JobState.PAUSED

@Composable
private fun SwipeToRemove(onRemove: () -> Unit, content: @Composable () -> Unit) {
    val state = rememberSwipeToDismissBoxState()
    LaunchedEffect(state.currentValue) {
        if (state.currentValue == SwipeToDismissBoxValue.EndToStart) onRemove()
    }
    val pad = cardPad()
    SwipeToDismissBox(
        state = state,
        enableDismissFromStartToEnd = false,
        backgroundContent = {
            val c = MaterialTheme.colorScheme
            Box(
                Modifier.fillMaxSize().padding(horizontal = pad).clip(RoundedCornerShape(LocalVgdStyle.current.groupRadius)).background(c.errorContainer).padding(end = 24.dp),
                contentAlignment = Alignment.CenterEnd,
            ) { Icon(Icons.Rounded.Delete, contentDescription = "Убрать", tint = c.onErrorContainer) }
        },
    ) { content() }
}

private fun stageName(s: String): String = when (s) {
    "queued" -> "В очереди"
    "info" -> "Получаю сведения"
    "video" -> "Видео"
    "audio" -> "Звук"
    "merge" -> "Склеиваю"
    "edit" -> "Копия для монтажа"
    "subs" -> "Субтитры"
    "wait" -> "Жду связь"
    "done" -> "Готово"
    "error" -> "Ошибка"
    "cancel" -> "Отменено"
    "paused" -> "Пауза"
    else -> s.replaceFirstChar { it.uppercase() }
}

/** «2/72» → «2 из 72»; «00:27» → «0:27». */
private fun itemOf(s: String) = s.split('/').takeIf { it.size == 2 }?.let { "${it[0]} из ${it[1]}" } ?: s
private fun eta(s: String) = s.replace(Regex("^0+(\\d:)"), "$1").replace(Regex("^0(\\d)"), "$1")

/** Короткая строка статуса: «YouTube · 2 из 72 · 3.7 МБ · 166 КБ/с · 0:27». */
fun statusLine(j: JobInfo): String {
    val pf = platformOf(j.url, j.audioOnly)
    val parts = mutableListOf(pf.name)
    when (j.state) {
        JobState.QUEUED -> parts += "В очереди"
        JobState.PAUSED -> parts += "Пауза"
        JobState.CANCELLED -> parts += "Отменено"
        JobState.ERROR -> parts += "Не скачалось"
        JobState.DONE -> parts += j.message.ifEmpty { "Готово" }.replaceFirstChar { it.uppercase() }
        JobState.RUNNING -> {
            if (j.item.isNotEmpty()) parts += itemOf(j.item)
            if ((j.stage == "video" || j.stage == "audio") && j.speed.isNotEmpty()) {
                j.message.substringAfterLast(" · ", "").takeIf { it.isNotEmpty() && it.first().isDigit() }?.let { parts += it }
                parts += j.speed
                if (j.eta.isNotEmpty()) parts += eta(j.eta)
            } else {
                parts += j.message.ifEmpty { stageName(j.stage) }.removeSuffix("…").replaceFirstChar { it.uppercase() }
            }
        }
    }
    return parts.distinct().joinToString(" · ")
}

/** Название карточки: пока его нет — короткая ссылка вместо полной. */
private fun cardTitle(j: JobInfo): String {
    val t = j.title.ifEmpty { j.url }
    if (!t.startsWith("http")) return t
    return t.substringAfter("://").removePrefix("www.").removePrefix("m.").let { if (it.length > 48) it.take(47) + "…" else it }
}

@Composable
fun JobCard(j: JobInfo, actions: QueueActions, lifted: Boolean, onRemove: () -> Unit) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val ctx = LocalContext.current
    val pf = platformOf(j.url, j.audioOnly)
    val square = pf.music || j.audioOnly
    var menu by remember { mutableStateOf(false) }
    KitCard(Modifier.fillMaxWidth().padding(horizontal = cardPad()).then(if (lifted) Modifier.shadow(12.dp, RoundedCornerShape(st.groupRadius)) else Modifier)) {
        Row(Modifier.padding(start = 12.dp, top = 12.dp, bottom = 12.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Thumb(thumbFor(j.url, j.thumb), pf, if (square) 64.dp else 112.dp, 64.dp, st.thumbRadius)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    cardTitle(j),
                    style = if (st.kind == Kind.YOU) MaterialTheme.typography.titleSmall else MaterialTheme.typography.titleMedium.copy(fontSize = MaterialTheme.typography.bodyLarge.fontSize * 0.94f),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(2.dp))
                Text(statusLine(j), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (j.state == JobState.RUNNING) {
                    Spacer(Modifier.height(8.dp))
                    KitProgress(if (j.percent > 0f) j.percent / 100f else null)
                }
                if (j.error.isNotEmpty()) {
                    Text(j.error, style = MaterialTheme.typography.bodySmall, color = c.error, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 4.dp))
                }
                if (j.notice.isNotEmpty()) {
                    Text(j.notice, style = MaterialTheme.typography.bodySmall, color = c.primary, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 4.dp))
                }
            }
            val tint = if (st.kind == Kind.IOS || st.kind == Kind.HARMONY) c.primary else c.onSurfaceVariant
            when (j.state) {
                JobState.QUEUED, JobState.RUNNING -> IconButton(onClick = { actions.pause(j.id) }) { Icon(Icons.Rounded.Pause, "Пауза", tint = tint) }
                JobState.PAUSED -> IconButton(onClick = { actions.resume(j.id) }) { Icon(Icons.Rounded.PlayArrow, "Продолжить", tint = tint) }
                JobState.ERROR, JobState.CANCELLED -> IconButton(onClick = { actions.retry(j.id) }) { Icon(Icons.Rounded.Refresh, "Повторить", tint = tint) }
                JobState.DONE -> Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
                    Icon(Icons.Rounded.CheckCircle, "Готово", tint = c.tertiary)
                }
            }
            Box {
                IconButton(onClick = { menu = true }) {
                    Icon(if (st.kind == Kind.IOS) Icons.Rounded.MoreHoriz else Icons.Rounded.MoreVert, "Ещё", tint = c.onSurfaceVariant)
                }
                DropdownMenu(menu, { menu = false }, shape = RoundedCornerShape(if (st.kind == Kind.YOU) 16.dp else 14.dp)) {
                    when (j.state) {
                        JobState.QUEUED, JobState.RUNNING -> DropdownMenuItem({ Text("Пауза") }, { menu = false; actions.pause(j.id) }, leadingIcon = { Icon(Icons.Rounded.Pause, null) })
                        JobState.PAUSED -> DropdownMenuItem({ Text("Продолжить") }, { menu = false; actions.resume(j.id) }, leadingIcon = { Icon(Icons.Rounded.PlayArrow, null) })
                        else -> DropdownMenuItem({ Text("Скачать ещё раз") }, { menu = false; actions.retry(j.id) }, leadingIcon = { Icon(Icons.Rounded.Refresh, null) })
                    }
                    DropdownMenuItem({ Text("Копировать ссылку") }, {
                        menu = false
                        (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("ссылка", j.url))
                    }, leadingIcon = { Icon(Icons.Rounded.Share, null) })
                    DropdownMenuItem(
                        { Text(if (j.isActive()) "Отменить и убрать" else "Убрать", color = c.error) },
                        { menu = false; onRemove() },
                        leadingIcon = { Icon(Icons.Rounded.Delete, null, tint = c.error) },
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
private fun ShareHintDialog(onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Rounded.IosShare, null) },
        title = { Text("Скачивать прямо из YouTube") },
        text = {
            Text(
                "1. В YouTube: «Поделиться» → «Ещё».\n" +
                    "2. Выберите «VGD: скачать» (или «VGD: звук» — только звук).\n" +
                    "3. Чтобы приложение всегда было сверху: в той же панели долго нажмите «VGD: скачать» → «Закрепить».\n\n" +
                    "В короткий ряд самого YouTube (Telegram, Discord…) сторонние приложения не попадают — его составляет YouTube.",
            )
        },
        confirmButton = { KitButton("Понятно", onDismiss, kind = Btn.TEXT) },
    )
}

// ---------- «Журнал» ----------

@Composable
private fun LogScreen(nav: @Composable () -> Unit) {
    val log by Vgd.log.collectAsState()
    LogContent(log, nav)
}

@Composable
fun LogContent(log: List<String>, nav: @Composable () -> Unit) {
    val ctx = LocalContext.current
    var find by rememberSaveable { mutableStateOf("") }
    val shown = remember(log, find) { if (find.isBlank()) log else log.filter { it.contains(find, ignoreCase = true) } }
    val state = rememberLazyListState()
    LaunchedEffect(shown.size) { if (shown.isNotEmpty()) state.scrollToItem(shown.size) }
    val c = MaterialTheme.colorScheme
    VgdScaffold(
        title = "Журнал",
        bottomBar = nav,
        actions = {
            // отправить журнал (например, в Telegram — разобраться, что не скачалось)
            IconButton(onClick = {
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, shown.takeLast(500).joinToString("\n"))
                ctx.startActivity(Intent.createChooser(send, "Журнал Very Good Downloader"))
            }) { Icon(Icons.Rounded.Share, contentDescription = "Отправить журнал") }
        },
    ) { inner ->
        SelectionContainer {
            LazyColumn(
                state = state,
                contentPadding = PaddingValues(top = inner.calculateTopPadding() + 4.dp, bottom = inner.calculateBottomPadding() + 16.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                item(key = "find") {
                    KitTextField(
                        find, { find = it }, "Поиск в журнале",
                        Modifier.padding(horizontal = cardPad()).padding(bottom = 8.dp),
                        singleLine = true,
                        trailing = { Icon(Icons.Rounded.Search, null) },
                    )
                }
                items(shown) { line ->
                    val color = when {
                        line.startsWith("Ошибка") -> c.error
                        line.startsWith("Внимание") || line.startsWith("Предупреждение") -> c.primary
                        else -> c.onSurface
                    }
                    Text(
                        line,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        lineHeight = 16.sp,
                        color = color,
                        modifier = Modifier.padding(horizontal = cardPad() + 4.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
}
