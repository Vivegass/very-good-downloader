@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)

package io.github.vivegass.vgd.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.vivegass.vgd.Vgd
import io.github.vivegass.vgd.core.ArtistInfo
import io.github.vivegass.vgd.core.CatalogHit
import io.github.vivegass.vgd.core.CatalogResult
import io.github.vivegass.vgd.core.artistInfo
import io.github.vivegass.vgd.core.artistLinkId
import io.github.vivegass.vgd.core.artistTopUrl
import io.github.vivegass.vgd.core.catalogSearch
import io.github.vivegass.vgd.core.discographyUrl
import io.github.vivegass.vgd.core.extractUrl
import io.github.vivegass.vgd.core.extractUrlOrSearch
import io.github.vivegass.vgd.text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** Обложка или фото исполнителя (кружок); пока грузится — серый квадрат. */
@Composable
private fun Cover(url: String, size: Dp, round: Boolean) {
    val img by produceState(if (url.isEmpty()) null else NetThumbs.cached(url), url) {
        if (value == null && url.isNotEmpty()) value = NetThumbs.load(url)
    }
    Box(
        Modifier.size(size).clip(if (round) CircleShape else RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.surfaceVariant),
    ) {
        img?.let { Image(it, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
    }
}

/** Текст в поле — название (не ссылка, одна строка, от 3 символов). */
fun isSearchText(text: String): Boolean {
    val q = text.trim()
    if (q.length < 3 || q.contains('\n')) return false
    return runCatching { extractUrl(q) == null && extractUrlOrSearch(q) != null }.getOrDefault(false)
}

/** Ссылка на исполнителя (Deezer, Spotify, Apple Music) — вместо мгновенной загрузки открываем дискографию. */
fun isArtistLink(text: String): Boolean {
    val u = text.trim().lowercase()
    return !u.contains('\n') && u.contains("/artist/") && !u.contains('#') &&
        (u.contains("deezer.com/") || u.contains("open.spotify.com/") || u.contains("music.apple.com/"))
}

/** Что открыть в листе дискографии: исполнитель каталога (id) или ссылка на него. */
data class DiscoReq(val id: Long?, val link: String?, val name: String, val picture: String)

/**
 * Выбор по названию: под полем — исполнители, альбомы и треки из каталога.
 * Трек и альбом — сразу в очередь, исполнитель — лист дискографии.
 */
@Composable
fun CatalogResults(text: String, onUrl: (String) -> Unit, onArtist: (DiscoReq) -> Unit) {
    val q = text.trim()
    val active = isSearchText(q)
    var res by remember { mutableStateOf<CatalogResult?>(null) }
    var err by remember { mutableStateOf<String?>(null) }
    var shownFor by remember { mutableStateOf("") }
    var closed by remember { mutableStateOf("") }
    var more by remember { mutableStateOf(false) }
    LaunchedEffect(q, active) {
        more = false
        if (!active) return@LaunchedEffect
        delay(450) // ждём, пока человек допечатает
        val r = withContext(Dispatchers.IO) {
            runCatching {
                Vgd.core()
                catalogSearch(q)
            }
        }
        res = r.getOrNull()
        err = r.exceptionOrNull()?.text()
        shownFor = q
    }
    if (!active || closed == q) return
    val c = MaterialTheme.colorScheme
    val ty = MaterialTheme.typography
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            val loading = shownFor != q
            if (loading) {
                CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                Spacer(Modifier.width(8.dp))
            }
            Text(if (loading) "Ищу в каталоге…" else "Что скачать?", style = ty.labelLarge, color = c.onSurfaceVariant, modifier = Modifier.weight(1f))
            IconButton(onClick = { closed = q }) { Icon(Icons.Rounded.Close, contentDescription = "Скрыть", tint = c.onSurfaceVariant) }
        }
        if (shownFor != q) return@Column
        val r = res
        when {
            r == null -> {
                Text("Каталог не ответил${err?.let { ": $it" } ?: ""}. «Скачать» всё равно найдёт трек.", style = ty.bodySmall, color = c.onSurfaceVariant)
                return@Column
            }
            r.artists.isEmpty() && r.albums.isEmpty() && r.tracks.isEmpty() -> {
                Text("В каталоге ничего не нашлось — «Скачать» поищет трек по названию.", style = ty.bodySmall, color = c.onSurfaceVariant)
                return@Column
            }
        }
        val sections = listOf(
            Triple("Исполнители", r!!.artists, if (more) 99 else 1),
            Triple("Альбомы", r.albums, if (more) 99 else 3),
            Triple("Треки", r.tracks, if (more) 99 else 4),
        )
        sections.forEach { (title, list, n) ->
            if (list.isEmpty()) return@forEach
            Text(title, style = ty.labelMedium, color = c.onSurfaceVariant, modifier = Modifier.padding(top = 6.dp))
            list.take(n).forEach { h -> HitRow(h) { if (h.kind == "artist") onArtist(DiscoReq(h.id, null, h.title, h.cover)) else onUrl(h.url) } }
        }
        val hidden = (r.artists.size - 1).coerceAtLeast(0) + (r.albums.size - 3).coerceAtLeast(0) + (r.tracks.size - 4).coerceAtLeast(0)
        if (!more && hidden > 0) TextButton(onClick = { more = true }) { Text("Ещё $hidden") }
    }
}

@Composable
private fun HitRow(h: CatalogHit, onClick: () -> Unit) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).clickable(onClick = onClick).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Cover(h.cover, 44.dp, h.kind == "artist")
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(h.title, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(h.sub, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Icon(
            if (h.kind == "artist") Icons.Rounded.ChevronRight else Icons.Rounded.Download,
            contentDescription = if (h.kind == "artist") "Дискография" else "Скачать",
            tint = c.primary,
        )
    }
}

private val KINDS = listOf("album" to "Альбомы", "ep" to "EP", "single" to "Синглы", "compile" to "Сборники")
private fun kindName(k: String) = when (k) {
    "ep" -> "EP"
    "single" -> "Сингл"
    "compile" -> "Сборник"
    else -> "Альбом"
}

/** Лист дискографии: типы релизов, концертные, без повторов, галочки у каждого релиза. */
@Composable
fun DiscoSheet(req: DiscoReq, onDismiss: () -> Unit, onDownload: (String) -> Unit, onSubscribe: (url: String, name: String) -> Unit) {
    val c = MaterialTheme.colorScheme
    val ty = MaterialTheme.typography
    var info by remember(req) { mutableStateOf<ArtistInfo?>(null) }
    var err by remember(req) { mutableStateOf<String?>(null) }
    var picked by remember(req) { mutableStateOf(listOf<Boolean>()) }
    var kinds by remember(req) { mutableStateOf(listOf(true, true, false, false)) }
    var live by remember(req) { mutableStateOf(false) }
    var nodup by remember(req) { mutableStateOf(true) }
    fun apply() {
        val a = info ?: return
        picked = a.releases.map { r ->
            val k = KINDS.indexOfFirst { it.first == r.kind }.coerceAtLeast(0)
            kinds[k] && (live || !r.live) && (!nodup || !r.dup)
        }
    }
    LaunchedEffect(req) {
        val r = withContext(Dispatchers.IO) {
            runCatching {
                Vgd.core()
                val id = req.id ?: req.link?.let { artistLinkId(it) } ?: error("не понял ссылку на исполнителя")
                artistInfo(id)
            }
        }
        info = r.getOrNull()
        err = r.exceptionOrNull()?.text()
        info?.let { a -> picked = a.releases.map { it.pick } }
    }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            val a = info
            Row(verticalAlignment = Alignment.CenterVertically) {
                Cover(a?.picture ?: req.picture, 52.dp, true)
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text(a?.name ?: req.name, style = ty.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text("Дискография · папки «год - альбом»", style = ty.bodySmall, color = c.onSurfaceVariant)
                }
            }
            when {
                a == null && err != null -> Text("Не вышло: $err", color = c.error, style = ty.bodyMedium)
                a == null -> Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text("Загружаю релизы…", style = ty.bodyMedium, color = c.onSurfaceVariant)
                }
                else -> {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        KINDS.forEachIndexed { i, (k, label) ->
                            val n = a.releases.count { it.kind == k }
                            if (n > 0) {
                                FilterChip(kinds[i], onClick = {
                                    kinds = kinds.toMutableList().also { it[i] = !it[i] }
                                    apply()
                                }, label = { Text("$label $n") })
                            }
                        }
                        val nl = a.releases.count { it.live }
                        if (nl > 0) FilterChip(live, onClick = { live = !live; apply() }, label = { Text("Концертные $nl") })
                        FilterChip(nodup, onClick = { nodup = !nodup; apply() }, label = { Text("Без повторов") })
                    }
                    LazyColumn(Modifier.weight(1f, fill = false).heightIn(max = 460.dp), contentPadding = PaddingValues(vertical = 4.dp)) {
                        itemsIndexed(a.releases, key = { _, r -> r.id }) { i, r ->
                            val on = picked.getOrElse(i) { false }
                            Row(
                                Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp))
                                    .clickable { picked = picked.toMutableList().also { if (i < it.size) it[i] = !on } }
                                    .padding(vertical = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Checkbox(on, onCheckedChange = null, modifier = Modifier.padding(horizontal = 8.dp))
                                Cover(r.cover, 40.dp, false)
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(r.title, style = ty.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    val marks = listOfNotNull(r.year.ifEmpty { null }, kindName(r.kind), "концертный".takeIf { r.live }, "другая версия".takeIf { r.dup })
                                    Text(marks.joinToString(" · "), style = ty.bodySmall, color = c.onSurfaceVariant)
                                }
                            }
                        }
                    }
                    val n = picked.count { it }
                    val subUrl = "https://www.deezer.com/artist/${a.id}"
                    Text("Выбрано: $n из ${a.releases.size}", style = ty.bodyMedium, color = c.onSurfaceVariant)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        TextButton(onClick = { onSubscribe(subUrl, a.name) }) { Text("Подписаться") }
                        TextButton(onClick = { onDownload(artistTopUrl(a.id)) }) { Text("Популярное") }
                        Spacer(Modifier.weight(1f))
                        KitButton("Скачать", {
                            val ids = a.releases.filterIndexed { i, _ -> picked.getOrElse(i) { false } }.map { it.id }
                            onDownload(discographyUrl(a.id, ids))
                        }, icon = Icons.Rounded.Download, enabled = n > 0)
                    }
                }
            }
        }
    }
}
