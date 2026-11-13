package io.github.vivegass.vgd.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.Subscriptions
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import io.github.vivegass.vgd.DownloadService
import io.github.vivegass.vgd.SubsWorker
import io.github.vivegass.vgd.Vgd
import io.github.vivegass.vgd.core.extractUrl
import io.github.vivegass.vgd.core.saveSettingsJson
import io.github.vivegass.vgd.core.settingsJson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Подписка для экрана. */
data class Sub(val url: String, val name: String, val audio: Boolean, val lastCheck: Long)

interface SubsActions {
    fun add(url: String, audio: Boolean) {}
    fun remove(url: String) {}
    fun check(url: String?) {}
    fun auto(v: Boolean) {}
}

/** Подписки: каналы и плейлисты, из которых качаются только новые видео (те же, что в окне на ПК). */
@Composable
internal fun SubsScreen(nav: @Composable () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var subs by remember { mutableStateOf<List<Sub>?>(null) }
    var auto by remember { mutableStateOf(SubsWorker.enabled(ctx)) }

    suspend fun reload() {
        subs = withContext(Dispatchers.IO) {
            Vgd.core()
            val a = JSONObject(settingsJson()).optJSONArray("subscriptions") ?: JSONArray()
            (0 until a.length()).map { a.getJSONObject(it) }.map {
                Sub(it.optString("url"), it.optString("name"), it.optBoolean("audio_only"), it.optLong("last_check"))
            }
        }
    }
    fun change(f: (JSONArray) -> Unit) {
        scope.launch(Dispatchers.IO) {
            val o = JSONObject(settingsJson())
            val a = o.optJSONArray("subscriptions") ?: JSONArray()
            f(a)
            o.put("subscriptions", a)
            saveSettingsJson(o.toString())
            Vgd.coreOrNull()?.reloadSettings()
            reload()
        }
    }
    LaunchedEffect(Unit) { runCatching { reload() } }
    val actions = object : SubsActions {
        override fun add(url: String, audio: Boolean) = change { a ->
            if ((0 until a.length()).none { a.getJSONObject(it).optString("url") == url }) {
                a.put(JSONObject().put("url", url).put("name", "").put("audio_only", audio).put("max_age_days", 0).put("seeded", false).put("last_check", 0))
            }
        }
        override fun remove(url: String) = change { a ->
            (0 until a.length()).firstOrNull { a.getJSONObject(it).optString("url") == url }?.let { a.remove(it) }
        }
        override fun check(url: String?) {
            DownloadService.start(ctx)
            act { it.checkSubscriptions(url) }
        }
        override fun auto(v: Boolean) {
            auto = v
            SubsWorker.schedule(ctx, v)
        }
    }
    SubsContent(subs, auto, actions, nav)
}

@Composable
fun SubsContent(subs: List<Sub>?, auto: Boolean, actions: SubsActions, nav: @Composable () -> Unit) {
    var url by remember { mutableStateOf("") }
    var audio by remember { mutableStateOf(false) }
    val pad = cardPad()
    VgdScaffold(title = "Подписки", subtitle = subs?.let { if (it.isEmpty()) null else "Каналов и плейлистов: ${it.size}" }, bottomBar = nav) { inner ->
        LazyColumn(
            contentPadding = PaddingValues(top = inner.calculateTopPadding() + 4.dp, bottom = inner.calculateBottomPadding() + 24.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            item(key = "add") {
                KitCard(Modifier.fillMaxWidth().padding(horizontal = pad)) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        KitTextField(url, { url = it }, "Ссылка на канал или плейлист", singleLine = true)
                        KitSegmented(listOf("Видео", "Только звук"), if (audio) 1 else 0, { audio = it == 1 })
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Spacer(Modifier.weight(1f))
                            KitButton("Подписаться", {
                                val u = extractUrl(url) ?: return@KitButton
                                url = ""
                                actions.add(u, audio)
                            }, icon = Icons.Rounded.Add, enabled = extractUrl(url) != null)
                        }
                    }
                }
            }
            item(key = "auto") {
                Group(footer = "Первая проверка только запоминает, что уже есть на канале; дальше качаются новые видео.") {
                    toggle("Проверять раз в 6 часов", auto, "Только по Wi-Fi и при заряженной батарее", Icons.Rounded.Schedule, Tint.GREEN) { actions.auto(it) }
                }
            }
            val list = subs
            if (list != null && list.isNotEmpty()) {
                item(key = "list") {
                    Group("Подписки") {
                        list.forEach { s ->
                            item { d ->
                                val last = if (s.lastCheck > 0) "Проверена " + SimpleDateFormat("d MMM, HH:mm", Locale.forLanguageTag("ru")).format(Date(s.lastCheck * 1000)) else "Ещё не проверялась"
                                KitRow(
                                    s.name.ifEmpty { s.url.substringAfter("://").removePrefix("www.") },
                                    subtitle = listOfNotNull(last, "только звук".takeIf { s.audio }).joinToString(" · "),
                                    icon = platformOf(s.url, s.audio).icon,
                                    tint = Tint.RED,
                                    divider = d,
                                ) {
                                    Row {
                                        IconButton(onClick = { actions.check(s.url) }) { Icon(Icons.Rounded.Refresh, contentDescription = "Проверить", tint = MaterialTheme.colorScheme.primary) }
                                        IconButton(onClick = { actions.remove(s.url) }) { Icon(Icons.Rounded.Close, contentDescription = "Отписаться", tint = MaterialTheme.colorScheme.onSurfaceVariant) }
                                    }
                                }
                            }
                        }
                    }
                }
                item(key = "check") {
                    Row(Modifier.fillMaxWidth().padding(horizontal = pad)) {
                        KitButton("Проверить все", { actions.check(null) }, Modifier.fillMaxWidth(), kind = Btn.TONAL, icon = Icons.Rounded.Refresh)
                    }
                }
            } else if (list != null) {
                item(key = "empty") {
                    EmptyState(Icons.Rounded.Subscriptions, "Подписок пока нет", "Вставьте ссылку на канал или плейлист — новые видео будут скачиваться сами.")
                }
            }
        }
    }
}
