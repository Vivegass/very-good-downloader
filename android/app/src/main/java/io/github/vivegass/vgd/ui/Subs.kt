package io.github.vivegass.vgd.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
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
import androidx.compose.ui.text.style.TextOverflow
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

/** Подписки: каналы и плейлисты, из которых качаются только новые видео (те же, что в окне на ПК). */
@Composable
internal fun SubsPage(pad: PaddingValues) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var subs by remember { mutableStateOf<JSONArray?>(null) }
    var url by remember { mutableStateOf("") }
    var audio by remember { mutableStateOf(false) }
    var auto by remember { mutableStateOf(SubsWorker.enabled(ctx)) }

    suspend fun reload() {
        subs = withContext(Dispatchers.IO) {
            Vgd.core()
            JSONObject(settingsJson()).optJSONArray("subscriptions") ?: JSONArray()
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
    fun check(only: String?) {
        DownloadService.start(ctx)
        act { it.checkSubscriptions(only) }
    }
    LaunchedEffect(Unit) { runCatching { reload() } }

    LazyColumn(
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = pad.calculateTopPadding() + 4.dp, bottom = pad.calculateBottomPadding() + 16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item {
            Text(
                "Первая проверка только запоминает, что уже есть на канале; дальше качаются новые видео.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        item {
            OutlinedTextField(url, { url = it }, label = { Text("ссылка на канал или плейлист") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = audio, onCheckedChange = { audio = it })
                Text("только звук", modifier = Modifier.weight(1f))
                Button(enabled = extractUrl(url) != null, onClick = {
                    val u = extractUrl(url) ?: return@Button
                    url = ""
                    change { a ->
                        if ((0 until a.length()).none { a.getJSONObject(it).optString("url") == u }) {
                            a.put(JSONObject().put("url", u).put("name", "").put("audio_only", audio).put("max_age_days", 0).put("seeded", false).put("last_check", 0))
                        }
                    }
                }) { Text("подписаться") }
            }
        }
        item {
            Toggle("проверять сами раз в 6 часов (только по Wi-Fi)", auto) { v ->
                auto = v
                SubsWorker.schedule(ctx, v)
            }
        }
        val list = subs
        if (list != null && list.length() > 0) {
            item { FilledTonalButton(onClick = { check(null) }) { Text("проверить все") } }
        }
        if (list != null) {
            itemsIndexed((0 until list.length()).map { list.getJSONObject(it) }) { i, s ->
                VgdCard {
                    Row(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(s.optString("name").ifEmpty { s.optString("url") }, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            val last = s.optLong("last_check")
                            val when_ = if (last > 0) SimpleDateFormat("d MMM HH:mm", Locale.forLanguageTag("ru")).format(Date(last * 1000)) else "ещё не проверялась"
                            Text(
                                listOfNotNull(when_, "только звук".takeIf { s.optBoolean("audio_only") }).joinToString(" · "),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        IconButton(onClick = { check(s.optString("url")) }) { Icon(Icons.Filled.Refresh, contentDescription = "проверить") }
                        IconButton(onClick = { change { a -> a.remove(i) } }) { Icon(Icons.Filled.Close, contentDescription = "отписаться") }
                    }
                }
            }
        }
    }
}
