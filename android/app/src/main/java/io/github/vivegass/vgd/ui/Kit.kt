@file:OptIn(ExperimentalMaterial3Api::class)

package io.github.vivegass.vgd.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.TopAppBarState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// ---------- цвета поверхностей по стилю ----------

/** Фон страницы. */
@Composable
fun pageColor(): Color {
    val c = MaterialTheme.colorScheme
    return when (LocalVgdStyle.current.kind) {
        // Android 16: фон настроек — тонированный контейнер, пункты светлее
        Kind.YOU -> if (c.isDark()) c.surface else c.surfaceContainer
        else -> c.background
    }
}

/** Фон группы пунктов / карточки. */
@Composable
fun groupColor(): Color {
    val c = MaterialTheme.colorScheme
    return when (LocalVgdStyle.current.kind) {
        Kind.YOU -> if (c.isDark()) c.surfaceContainerHigh else c.surfaceBright
        Kind.HYPER -> c.background // плоский список прямо на фоне
        else -> c.surfaceContainer
    }
}

/** Фон отдельной карточки (очередь). */
@Composable
fun cardColor(): Color = when (LocalVgdStyle.current.kind) {
    Kind.HYPER -> MaterialTheme.colorScheme.surfaceContainer
    else -> groupColor()
}

/** Поле ввода, дорожка переключателя. */
@Composable
fun fieldColor(): Color = MaterialTheme.colorScheme.surfaceContainerHighest

// ---------- заголовок экрана: крупный, сжимается при прокрутке ----------

private data class HeaderSpec(val expanded: Dp, val collapsed: Dp)

@Composable
private fun headerSpec(kind: Kind): HeaderSpec {
    val h = LocalConfiguration.current.screenHeightDp
    return when (kind) {
        Kind.YOU -> HeaderSpec(152.dp, 64.dp)
        // One UI: зона просмотра — верхняя треть экрана
        Kind.ONEUI -> HeaderSpec((h * 0.32f).coerceIn(200f, 320f).dp, 56.dp)
        // iOS: навбар 44 + Large Title 52
        Kind.IOS -> HeaderSpec(96.dp, 44.dp)
        Kind.HYPER -> HeaderSpec(132.dp, 56.dp)
        Kind.HARMONY -> HeaderSpec(112.dp, 56.dp)
    }
}

/**
 * Каркас экрана: заголовок стиля (сжимается при прокрутке и не налезает на содержимое: высота шапки входит
 * в отступ содержимого), навигация стиля (плавающая тоже входит в нижний отступ), Snackbar.
 * Содержимое получает отступы — их нужно отдать списку как contentPadding.
 */
@Composable
fun VgdScaffold(
    title: String,
    subtitle: String? = null,
    bottomBar: @Composable () -> Unit = {},
    snackbar: SnackbarHostState? = null,
    actions: @Composable RowScope.() -> Unit = {},
    content: @Composable (PaddingValues) -> Unit,
) {
    val state = rememberTopAppBarState()
    val behavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(state)
    Scaffold(
        modifier = Modifier.nestedScroll(behavior.nestedScrollConnection),
        containerColor = pageColor(),
        topBar = { Header(title, subtitle, state, actions) },
        bottomBar = bottomBar,
        snackbarHost = { if (snackbar != null) SnackbarHost(snackbar) },
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
    ) { pad -> content(pad) }
}

@Composable
private fun Header(title: String, subtitle: String?, state: TopAppBarState, actions: @Composable RowScope.() -> Unit) {
    val st = LocalVgdStyle.current
    val spec = headerSpec(st.kind)
    val density = LocalDensity.current
    val limit = with(density) { (spec.collapsed - spec.expanded).toPx() }
    SideEffect { if (state.heightOffsetLimit != limit) state.heightOffsetLimit = limit }
    val f = state.collapsedFraction
    val height = spec.expanded + with(density) { state.heightOffset.toDp() }
    val page = pageColor()
    val c = MaterialTheme.colorScheme
    val bg = when (st.kind) {
        Kind.YOU -> lerp(page, if (c.isDark()) c.surfaceContainer else c.surfaceContainerHigh, f)
        // «материал» навбара iOS / стекло HyperOS: чуть прозрачный, когда содержимое уходит под него
        Kind.IOS, Kind.HYPER -> page.copy(alpha = 1f - 0.08f * f)
        else -> page
    }
    val ty = MaterialTheme.typography
    Column(Modifier.fillMaxWidth().background(bg).windowInsetsPadding(WindowInsets.statusBars)) {
        Box(Modifier.fillMaxWidth().height(height)) {
            // строка свёрнутого заголовка + действия
            Row(
                Modifier.fillMaxWidth().height(spec.collapsed).padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val small = when (st.kind) {
                    Kind.IOS -> ((f - 0.85f) / 0.15f).coerceIn(0f, 1f)
                    else -> ((f - 0.55f) / 0.45f).coerceIn(0f, 1f)
                }
                val centered = st.kind == Kind.IOS || st.kind == Kind.HYPER
                Box(Modifier.weight(1f).padding(start = if (centered) 48.dp else 12.dp, end = if (centered) 0.dp else 0.dp)) {
                    Text(
                        title,
                        style = when (st.kind) {
                            Kind.IOS -> ty.titleMedium
                            Kind.YOU -> ty.titleLarge
                            else -> ty.titleLarge
                        },
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        textAlign = if (centered) TextAlign.Center else TextAlign.Start,
                        modifier = Modifier.fillMaxWidth().alpha(small),
                    )
                }
                Row(Modifier.widthIn(min = if (centered) 48.dp else 0.dp), horizontalArrangement = Arrangement.End, content = actions)
            }
            // крупный заголовок
            val big = (1f - f * 1.6f).coerceIn(0f, 1f)
            if (big > 0f) {
                when (st.kind) {
                    Kind.ONEUI -> Column(
                        Modifier.fillMaxSize().padding(top = spec.collapsed / 2, bottom = spec.collapsed / 2).alpha(big),
                        verticalArrangement = Arrangement.Center,
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text(title, style = ty.displaySmall, textAlign = TextAlign.Center)
                        if (!subtitle.isNullOrEmpty()) {
                            Spacer(Modifier.height(6.dp))
                            Text(subtitle, style = ty.bodyMedium, color = c.onSurfaceVariant, textAlign = TextAlign.Center)
                        }
                    }
                    else -> Column(
                        Modifier.align(Alignment.BottomStart).padding(
                            start = when (st.kind) { Kind.HYPER -> 24.dp; Kind.YOU -> 16.dp; else -> 16.dp },
                            bottom = when (st.kind) { Kind.IOS -> 6.dp; Kind.YOU -> 18.dp; else -> 12.dp },
                            end = 16.dp,
                        ).alpha(big),
                    ) {
                        Text(title, style = if (st.kind == Kind.YOU) ty.headlineLarge else ty.displaySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (!subtitle.isNullOrEmpty() && st.kind == Kind.YOU) {
                            Text(subtitle, style = ty.bodyMedium, color = c.onSurfaceVariant)
                        }
                    }
                }
            }
        }
        if ((st.kind == Kind.IOS || st.kind == Kind.HYPER) && f > 0.97f) {
            HorizontalDivider(thickness = 0.5.dp, color = c.outlineVariant)
        }
    }
}

// ---------- навигация ----------

data class NavItem(val label: String, val icon: ImageVector, val selectedIcon: ImageVector, val badge: Int = 0)

@Composable
fun VgdNavBar(items: List<NavItem>, selected: Int, onSelect: (Int) -> Unit) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    fun label(i: NavItem) = if (i.badge > 0) "${i.label} · ${i.badge}" else i.label
    when (st.kind) {
        Kind.YOU -> NavigationBar(containerColor = if (c.isDark()) c.surfaceContainer else c.surfaceContainerHigh) {
            items.forEachIndexed { i, it ->
                NavigationBarItem(
                    selected = i == selected,
                    onClick = { onSelect(i) },
                    icon = { Icon(if (i == selected) it.selectedIcon else it.icon, contentDescription = null) },
                    label = { Text(label(it), maxLines = 1) },
                    colors = NavigationBarItemDefaults.colors(
                        indicatorColor = c.primaryContainer,
                        selectedIconColor = c.onPrimaryContainer,
                        selectedTextColor = c.onSurface,
                    ),
                )
            }
        }
        // iOS 26: плавающая «стеклянная» капсула, выбранная вкладка — светлая капсула внутри, цвет — systemBlue
        Kind.IOS -> Box(
            Modifier.fillMaxWidth().background(fadeUnder()).windowInsetsPadding(WindowInsets.navigationBars).padding(start = 20.dp, end = 20.dp, bottom = 6.dp, top = 6.dp),
            contentAlignment = Alignment.Center,
        ) {
            Surface(
                shape = CircleShape,
                color = c.surfaceContainer.copy(alpha = 0.96f),
                border = BorderStroke(0.5.dp, c.outlineVariant.copy(alpha = 0.7f)),
                shadowElevation = 10.dp,
            ) {
                Row(Modifier.padding(4.dp)) {
                    items.forEachIndexed { i, it ->
                        val sel = i == selected
                        Column(
                            Modifier.weight(1f).clip(CircleShape)
                                .background(if (sel) c.onSurface.copy(alpha = 0.07f) else Color.Transparent)
                                .clickable { onSelect(i) }.padding(vertical = 6.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Icon(if (sel) it.selectedIcon else it.icon, contentDescription = null, tint = if (sel) c.primary else c.onSurface, modifier = Modifier.size(24.dp))
                            Text(it.label, fontSize = 10.sp, fontWeight = FontWeight.SemiBold, color = if (sel) c.primary else c.onSurface, maxLines = 1)
                        }
                    }
                }
            }
        }
        // One UI: ровная панель на фоне, выбранная — жирная подпись и залитый значок, без индикатора
        Kind.ONEUI -> Surface(color = pageColor()) {
            Row(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.navigationBars).height(64.dp)) {
                items.forEachIndexed { i, it ->
                    val sel = i == selected
                    Column(
                        Modifier.weight(1f).fillMaxHeight().clickable { onSelect(i) },
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        Icon(if (sel) it.selectedIcon else it.icon, contentDescription = null, tint = if (sel) c.onSurface else c.onSurfaceVariant)
                        Spacer(Modifier.height(2.dp))
                        Text(
                            label(it),
                            fontSize = 12.sp,
                            fontWeight = if (sel) FontWeight.Bold else FontWeight.Normal,
                            color = if (sel) c.onSurface else c.onSurfaceVariant,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
        // HyperOS: широкая плавающая полупрозрачная панель, выбранное — синим
        Kind.HYPER -> Box(Modifier.fillMaxWidth().background(fadeUnder()).windowInsetsPadding(WindowInsets.navigationBars).padding(horizontal = 14.dp, vertical = 8.dp)) {
            Surface(
                shape = RoundedCornerShape(28.dp),
                color = c.surfaceContainer.copy(alpha = 0.97f),
                shadowElevation = 8.dp,
                border = BorderStroke(0.5.dp, c.outlineVariant),
            ) {
                Row(Modifier.fillMaxWidth().height(60.dp)) {
                    items.forEachIndexed { i, it ->
                        val sel = i == selected
                        Column(
                            Modifier.weight(1f).fillMaxHeight().clickable { onSelect(i) },
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.Center,
                        ) {
                            Icon(if (sel) it.selectedIcon else it.icon, contentDescription = null, tint = if (sel) c.primary else c.onSurfaceVariant)
                            Text(it.label, fontSize = 10.sp, color = if (sel) c.primary else c.onSurfaceVariant, maxLines = 1)
                        }
                    }
                }
            }
        }
        // HarmonyOS: значок над подписью, выбранное — в цвет акцента; панель полупрозрачная с тонкой линией сверху
        Kind.HARMONY -> Column(Modifier.background(pageColor().copy(alpha = 0.96f))) {
            HorizontalDivider(thickness = 0.5.dp, color = c.outlineVariant)
            Row(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.navigationBars).height(56.dp)) {
                items.forEachIndexed { i, it ->
                    val sel = i == selected
                    Column(
                        Modifier.weight(1f).fillMaxHeight().clickable { onSelect(i) },
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        Icon(if (sel) it.selectedIcon else it.icon, contentDescription = null, tint = if (sel) c.primary else c.onSurfaceVariant)
                        Text(label(it), fontSize = 10.sp, fontWeight = FontWeight.Medium, color = if (sel) c.primary else c.onSurfaceVariant, maxLines = 1)
                    }
                }
            }
        }
    }
}

/**
 * Под плавающей панелью содержимое уходит в фон страницы (как «край прокрутки» iOS 26): текст списка
 * не просвечивает вокруг и под капсулой, а последний пункт списка всё равно виден над панелью (отступ снизу).
 */
@Composable
private fun fadeUnder(): androidx.compose.ui.graphics.Brush {
    val p = pageColor()
    return androidx.compose.ui.graphics.Brush.verticalGradient(0f to p.copy(alpha = 0f), 0.35f to p.copy(alpha = 0.92f), 1f to p)
}

// ---------- группы и строки ----------

/** Цвет значка в строке (плашки iOS / HyperOS — системные цвета). */
enum class Tint(val light: Long, val dark: Long) {
    BLUE(0xFF007AFF, 0xFF0A84FF), GREEN(0xFF34C759, 0xFF30D158), ORANGE(0xFFFF9500, 0xFFFF9F0A), RED(0xFFFF3B30, 0xFFFF453A),
    PURPLE(0xFFAF52DE, 0xFFBF5AF2), PINK(0xFFFF2D55, 0xFFFF375F), TEAL(0xFF30B0C7, 0xFF40C8E0), INDIGO(0xFF5856D6, 0xFF5E5CE6),
    GRAY(0xFF8E8E93, 0xFF8E8E93), YELLOW(0xFFFFCC00, 0xFFFFD60A),
}

class GroupScope {
    internal val items = mutableListOf<@Composable (divider: Boolean) -> Unit>()
    fun item(content: @Composable (divider: Boolean) -> Unit) {
        items += content
    }
}

/** Заголовок группы / раздела. */
@Composable
fun SectionHeader(title: String, modifier: Modifier = Modifier, trailing: @Composable RowScope.() -> Unit = {}) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val ty = MaterialTheme.typography
    val (style, color, start) = when (st.kind) {
        Kind.YOU -> Triple(ty.titleSmall, c.primary, 16.dp)
        Kind.IOS -> Triple(ty.bodySmall, c.onSurfaceVariant, 16.dp)
        Kind.ONEUI -> Triple(ty.bodyMedium.copy(fontWeight = FontWeight.Bold), c.onSurfaceVariant, 20.dp)
        Kind.HYPER -> Triple(ty.bodySmall, c.onSurfaceVariant, 24.dp)
        Kind.HARMONY -> Triple(ty.bodyMedium.copy(fontWeight = FontWeight.Medium), c.onSurfaceVariant, 12.dp)
    }
    Row(modifier.fillMaxWidth().padding(start = start, end = 8.dp, top = 12.dp, bottom = 6.dp).heightIn(min = 24.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(if (st.upperSection) title.uppercase() else title, style = style, color = color, modifier = Modifier.weight(1f))
        trailing()
    }
}

/**
 * Группа пунктов: сегменты M3 Expressive (крупное скругление снаружи, малое между пунктами), сплошная группа
 * с разделителями (iOS, One UI, HarmonyOS — с тенью) или плоский список (HyperOS).
 */
@Composable
fun Group(title: String? = null, footer: String? = null, modifier: Modifier = Modifier, content: GroupScope.() -> Unit) {
    val st = LocalVgdStyle.current
    val scope = GroupScope().apply(content)
    val n = scope.items.size
    Column(modifier.fillMaxWidth().padding(horizontal = st.hPad)) {
        if (title != null) SectionHeader(title)
        when (st.kind) {
            Kind.YOU -> Column(verticalArrangement = Arrangement.spacedBy(st.itemGap)) {
                scope.items.forEachIndexed { i, item ->
                    val top = if (i == 0) st.groupRadius else st.innerRadius
                    val bottom = if (i == n - 1) st.groupRadius else st.innerRadius
                    Surface(shape = RoundedCornerShape(top, top, bottom, bottom), color = groupColor()) { item(false) }
                }
            }
            Kind.HYPER -> Column { scope.items.forEach { it(false) } }
            else -> Surface(
                shape = RoundedCornerShape(st.groupRadius),
                color = groupColor(),
                shadowElevation = st.shadow,
            ) {
                Column { scope.items.forEachIndexed { i, item -> item(i < n - 1) } }
            }
        }
        if (footer != null) {
            Text(
                footer,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 6.dp),
            )
        }
    }
}

/** Значок строки по стилю. */
@Composable
fun RowIcon(icon: ImageVector, tint: Tint) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val color = Color(if (c.isDark()) tint.dark else tint.light)
    when (st.kind) {
        // iOS Settings: белый значок на цветной скруглённой плашке 29 pt
        Kind.IOS -> Box(Modifier.size(30.dp).clip(RoundedCornerShape(7.dp)).background(color), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(19.dp))
        }
        // HyperOS: крупная цветная «скруглённая» плашка
        Kind.HYPER -> Box(Modifier.size(38.dp).clip(RoundedCornerShape(12.dp)).background(color), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(22.dp))
        }
        // HarmonyOS: значок цвета акцента в светлом круге
        Kind.HARMONY -> Box(Modifier.size(36.dp).clip(CircleShape).background(c.primary.copy(alpha = 0.1f)), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = c.primary, modifier = Modifier.size(20.dp))
        }
        // Material You: значок без плашки
        Kind.YOU -> Icon(icon, contentDescription = null, tint = c.onSurfaceVariant, modifier = Modifier.size(24.dp))
        // One UI: в подпунктах настроек значков нет
        Kind.ONEUI -> {}
    }
}

private fun iconWidth(kind: Kind): Dp = when (kind) {
    Kind.IOS -> 30.dp + 14.dp
    Kind.HYPER -> 38.dp + 16.dp
    Kind.HARMONY -> 36.dp + 12.dp
    Kind.YOU -> 24.dp + 16.dp
    Kind.ONEUI -> 0.dp
}

/**
 * Строка списка: вся нажимается. `value` — текущее значение: у One UI — синим под названием, у M3 — подписью,
 * у iOS / HyperOS / HarmonyOS — серым справа (с › у перехода).
 */
@Composable
fun KitRow(
    title: String,
    subtitle: String? = null,
    icon: ImageVector? = null,
    tint: Tint = Tint.BLUE,
    value: String? = null,
    divider: Boolean = false,
    nav: Boolean = false,
    enabled: Boolean = true,
    titleColor: Color = Color.Unspecified,
    onClick: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
    trailing: (@Composable () -> Unit)? = null,
) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val ty = MaterialTheme.typography
    val showIcon = icon != null && st.kind != Kind.ONEUI
    val valueRight = value != null && st.kind in setOf(Kind.IOS, Kind.HYPER, Kind.HARMONY)
    val hpad = when (st.kind) { Kind.HYPER -> 24.dp; Kind.ONEUI -> 22.dp; else -> 16.dp }
    Column(modifier.alpha(if (enabled) 1f else 0.45f)) {
        Row(
            Modifier.fillMaxWidth()
                .then(if (onClick != null && enabled) Modifier.clickable(onClick = onClick) else Modifier)
                .heightIn(min = if (subtitle != null || (value != null && !valueRight)) st.rowHeight else (st.rowHeight - 8.dp).coerceAtLeast(44.dp))
                .padding(horizontal = hpad, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showIcon) {
                RowIcon(icon!!, tint)
                Spacer(Modifier.width(iconWidth(st.kind) - when (st.kind) { Kind.IOS -> 30.dp; Kind.HYPER -> 38.dp; Kind.HARMONY -> 36.dp; else -> 24.dp }))
            }
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    style = when (st.kind) { Kind.HYPER, Kind.HARMONY -> ty.titleMedium; else -> ty.bodyLarge },
                    color = if (titleColor != Color.Unspecified) titleColor else c.onSurface,
                )
                val sub = listOfNotNull(if (!valueRight) value else null, subtitle)
                sub.forEachIndexed { k, s ->
                    val isValue = k == 0 && value != null && !valueRight
                    Text(
                        s,
                        style = if (st.kind == Kind.YOU || st.kind == Kind.ONEUI) ty.bodyMedium else ty.bodySmall,
                        color = if (isValue && st.summaryAccent) c.primary else c.onSurfaceVariant,
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
            }
            if (valueRight) {
                Spacer(Modifier.width(8.dp))
                Text(value!!, style = ty.bodyMedium, color = c.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 160.dp))
            }
            if (trailing != null) {
                Spacer(Modifier.width(12.dp))
                trailing()
            } else if (nav && st.chevron) {
                Icon(
                    Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = null,
                    tint = c.onSurfaceVariant.copy(alpha = 0.6f),
                    modifier = Modifier.padding(start = 4.dp).size(22.dp),
                )
            }
        }
        if (divider) {
            HorizontalDivider(
                thickness = 0.5.dp,
                color = c.outlineVariant,
                modifier = Modifier.padding(start = hpad + if (showIcon) iconWidth(st.kind) else 0.dp),
            )
        }
    }
}

/** Пункт группы: переход / действие. */
fun GroupScope.nav(
    title: String,
    subtitle: String? = null,
    icon: ImageVector? = null,
    tint: Tint = Tint.BLUE,
    value: String? = null,
    enabled: Boolean = true,
    onClick: () -> Unit,
) = item { d -> KitRow(title, subtitle, icon, tint, value, d, nav = true, enabled = enabled, onClick = onClick) }

/** Пункт группы: переключатель (нажимается вся строка). */
fun GroupScope.toggle(
    title: String,
    checked: Boolean,
    subtitle: String? = null,
    icon: ImageVector? = null,
    tint: Tint = Tint.BLUE,
    enabled: Boolean = true,
    onChange: (Boolean) -> Unit,
) = item { d ->
    KitRow(
        title, subtitle, icon, tint, null, d,
        enabled = enabled,
        modifier = Modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onChange),
    ) { KitSwitch(checked) }
}

/** Отступ содержимого строки от края группы (по стилю). */
@Composable
fun rowPad(): Dp = when (LocalVgdStyle.current.kind) { Kind.HYPER -> 24.dp; Kind.ONEUI -> 22.dp; else -> 16.dp }

/** Пункт группы: что угодно. */
fun GroupScope.custom(content: @Composable () -> Unit) = item { _ -> content() }

// ---------- переключатель ----------

@Composable
fun KitSwitch(checked: Boolean, onChange: ((Boolean) -> Unit)? = null) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val dark = c.isDark()
    when (st.kind) {
        Kind.YOU -> Switch(
            checked = checked,
            onCheckedChange = onChange,
            thumbContent = if (checked) {
                { Icon(Icons.Filled.Check, contentDescription = null, modifier = Modifier.size(SwitchDefaults.IconSize)) }
            } else null,
        )
        Kind.IOS -> PillSwitch(checked, onChange, 51.dp, 31.dp, 27.dp, Color(if (dark) 0xFF30D158 else 0xFF34C759), Color(if (dark) 0xFF39393D else 0xFFE9E9EA), Color.White, Color.White, null, 3.dp)
        Kind.ONEUI -> PillSwitch(checked, onChange, 46.dp, 26.dp, 20.dp, c.primary, if (dark) Color(0xFF3A3A3A) else Color(0xFFDCDCDC), Color.White, Color.White, null, 1.dp)
        Kind.HYPER -> PillSwitch(checked, onChange, 50.dp, 28.dp, 22.dp, c.primary, if (dark) Color(0xFF333333) else Color(0xFFE6E6E6), Color.White, if (dark) Color(0xFFBBBBBB) else Color.White, null, 1.dp)
        Kind.HARMONY -> PillSwitch(checked, onChange, 40.dp, 24.dp, 20.dp, c.primary, if (dark) Color(0x33FFFFFF) else Color(0x1A000000), Color.White, Color.White, null, 1.dp)
    }
}

@Composable
private fun PillSwitch(
    checked: Boolean, onChange: ((Boolean) -> Unit)?, w: Dp, h: Dp, thumb: Dp, on: Color, off: Color,
    thumbOn: Color, thumbOff: Color, offBorder: Color?, thumbShadow: Dp,
) {
    val pos by animateFloatAsState(if (checked) 1f else 0f, label = "switch")
    val pad = (h - thumb) / 2
    Box(
        Modifier.size(w, h).clip(CircleShape).background(lerp(off, on, pos))
            .then(if (offBorder != null && !checked) Modifier.border(1.dp, offBorder, CircleShape) else Modifier)
            .then(if (onChange != null) Modifier.toggleable(checked, role = Role.Switch, onValueChange = onChange) else Modifier),
    ) {
        Box(
            Modifier.offset(x = pad + (w - thumb - pad * 2) * pos, y = pad).size(thumb)
                .shadow(thumbShadow, CircleShape).clip(CircleShape).background(lerp(thumbOff, thumbOn, pos)),
        )
    }
}

// ---------- кнопки ----------

enum class Btn { PRIMARY, TONAL, TEXT }

@Composable
fun KitButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: Btn = Btn.PRIMARY,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val content: @Composable RowScope.() -> Unit = {
        if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
        }
        Text(text, maxLines = 1)
    }
    if (st.kind == Kind.YOU) {
        when (kind) {
            Btn.PRIMARY -> Button(onClick, modifier.heightIn(min = 52.dp), enabled = enabled, contentPadding = PaddingValues(horizontal = 24.dp), content = content)
            Btn.TONAL -> FilledTonalButton(onClick, modifier.heightIn(min = 44.dp), enabled = enabled, content = content)
            Btn.TEXT -> TextButton(onClick, modifier, enabled = enabled, content = content)
        }
        return
    }
    val (shape, height) = when (st.kind) {
        Kind.ONEUI -> RoundedCornerShape(24.dp) to 48.dp
        Kind.HARMONY -> CircleShape to 40.dp
        else -> CircleShape to 50.dp
    }
    val (bg, fg) = when (kind) {
        Btn.PRIMARY -> c.primary to c.onPrimary
        Btn.TONAL -> when (st.kind) {
            Kind.ONEUI -> fieldColor() to c.onSurface
            Kind.HARMONY -> c.onSurface.copy(alpha = 0.05f) to c.primary
            else -> c.primary.copy(alpha = 0.12f) to c.primary
        }
        Btn.TEXT -> Color.Transparent to c.primary
    }
    val weight = when (st.kind) { Kind.ONEUI -> FontWeight.Bold; Kind.IOS -> if (kind == Btn.TEXT) FontWeight.Normal else FontWeight.SemiBold; else -> FontWeight.Medium }
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = shape,
        color = bg,
        contentColor = fg,
        modifier = modifier.heightIn(min = if (kind == Btn.TEXT) 36.dp else if (kind == Btn.TONAL) height - 6.dp else height).alpha(if (enabled) 1f else 0.4f),
    ) {
        Row(
            Modifier.padding(horizontal = if (kind == Btn.TEXT) 10.dp else 20.dp),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            androidx.compose.material3.ProvideTextStyle(MaterialTheme.typography.labelLarge.copy(fontWeight = weight, color = fg)) { content() }
        }
    }
}

// ---------- выбор из нескольких ----------

/** Segmented: M3 SegmentedButton; iOS — segmented control; One UI — таблетки; HyperOS / HarmonyOS — дорожка с плашкой. */
@Composable
fun KitSegmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    if (st.kind == Kind.YOU) {
        SingleChoiceSegmentedButtonRow(modifier.fillMaxWidth()) {
            options.forEachIndexed { i, s ->
                SegmentedButton(
                    selected = i == selected,
                    onClick = { onSelect(i) },
                    shape = SegmentedButtonDefaults.itemShape(i, options.size),
                    colors = SegmentedButtonDefaults.colors(activeContainerColor = c.secondaryContainer),
                ) { Text(s, maxLines = 1) }
            }
        }
        return
    }
    val dark = c.isDark()
    val (track, selBg, selFg, radius, h) = when (st.kind) {
        Kind.IOS -> Seg(Color(0x1F767680), if (dark) Color(0xFF636366) else Color.White, c.onSurface, 9.dp, 30.dp)
        Kind.ONEUI -> Seg(fieldColor(), c.primary, c.onPrimary, 20.dp, 38.dp)
        Kind.HYPER -> Seg(fieldColor(), if (dark) Color(0xFF3A3A3A) else Color.White, c.primary, 14.dp, 38.dp)
        else -> Seg(fieldColor(), if (dark) Color(0xFF3A3A3A) else Color.White, c.primary, 20.dp, 34.dp)
    }
    Row(modifier.fillMaxWidth().clip(RoundedCornerShape(radius)).background(track).padding(2.dp)) {
        options.forEachIndexed { i, s ->
            val sel = i == selected
            Box(
                Modifier.weight(1f).height(h - 4.dp)
                    .then(if (sel && st.kind != Kind.ONEUI) Modifier.shadow(2.dp, RoundedCornerShape(radius - 2.dp)) else Modifier)
                    .clip(RoundedCornerShape(radius - 2.dp))
                    .background(if (sel) selBg else Color.Transparent)
                    .clickable { onSelect(i) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    s,
                    style = MaterialTheme.typography.labelMedium.copy(
                        fontSize = if (st.kind == Kind.IOS) 13.sp else 14.sp,
                        fontWeight = if (sel) FontWeight.SemiBold else FontWeight.Medium,
                    ),
                    color = if (sel) selFg else c.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
    }
}

private data class Seg(val track: Color, val selBg: Color, val selFg: Color, val radius: Dp, val h: Dp)

/** Вариант для листа выбора. */
data class Choice(val key: String, val label: String, val hint: String? = null, val preview: (@Composable () -> Unit)? = null)

/** Лист выбора снизу (как выпадающий список на ПК): у iOS — ✓ справа, у остальных — радио / галочка. */
@Composable
fun ChoiceSheet(title: String, choices: List<Choice>, selected: String, onSelect: (String) -> Unit, onDismiss: () -> Unit) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = choices.size > 6),
        containerColor = if (st.kind == Kind.YOU) c.surfaceContainerLow else pageColor(),
        shape = RoundedCornerShape(topStart = if (st.kind == Kind.IOS) 14.dp else 28.dp, topEnd = if (st.kind == Kind.IOS) 14.dp else 28.dp),
    ) {
        Text(
            title,
            style = MaterialTheme.typography.titleLarge,
            textAlign = if (st.kind == Kind.IOS) TextAlign.Center else TextAlign.Start,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(bottom = 12.dp),
        )
        androidx.compose.foundation.lazy.LazyColumn(contentPadding = PaddingValues(bottom = 32.dp)) {
            item {
                Group {
                    choices.forEach { ch ->
                        item { d ->
                            val sel = ch.key == selected
                            KitRow(
                                ch.label,
                                subtitle = ch.hint,
                                divider = d,
                                onClick = { onSelect(ch.key) },
                                trailing = {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        ch.preview?.invoke()
                                        if (st.kind == Kind.YOU) {
                                            Spacer(Modifier.width(8.dp))
                                            RadioButton(selected = sel, onClick = null)
                                        } else {
                                            Box(Modifier.size(28.dp), contentAlignment = Alignment.Center) {
                                                if (sel) Icon(Icons.Filled.Check, contentDescription = "выбрано", tint = c.primary)
                                            }
                                        }
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

// ---------- прогресс ----------

/**
 * Полоса прогресса без разрывов (у M3 1.4 между заполненной частью и дорожкой зазор и точка в конце — на телефоне
 * это выглядело «рваным»). null — неизвестно сколько.
 */
@Composable
fun KitProgress(fraction: Float?, modifier: Modifier = Modifier) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    val h = if (st.kind == Kind.ONEUI) 6.dp else 4.dp
    val track = when (st.kind) {
        Kind.YOU -> c.secondaryContainer
        Kind.IOS -> Color(0x33787880)
        else -> fieldColor()
    }
    BoxWithConstraints(modifier.fillMaxWidth().height(h).clip(CircleShape).background(track)) {
        if (fraction != null) {
            val f by animateFloatAsState(fraction.coerceIn(0f, 1f), label = "progress")
            Box(Modifier.fillMaxHeight().fillMaxWidth(f).clip(CircleShape).background(c.primary))
        } else {
            val tr = rememberInfiniteTransition(label = "busy")
            val x by tr.animateFloat(-0.3f, 1f, infiniteRepeatable(tween(1200, easing = LinearEasing), RepeatMode.Restart), label = "x")
            Box(Modifier.fillMaxHeight().width(maxWidth * 0.3f).offset(x = maxWidth * x).clip(CircleShape).background(c.primary))
        }
    }
}

// ---------- карточки, поля, пустое состояние ----------

@Composable
fun KitCard(modifier: Modifier = Modifier, color: Color = cardColor(), onClick: (() -> Unit)? = null, content: @Composable ColumnScope.() -> Unit) {
    val st = LocalVgdStyle.current
    val shape = RoundedCornerShape(if (st.kind == Kind.YOU) 20.dp else st.groupRadius)
    if (onClick != null) {
        Surface(onClick = onClick, shape = shape, color = color, shadowElevation = st.shadow, modifier = modifier) { Column(content = content) }
    } else {
        Surface(shape = shape, color = color, shadowElevation = st.shadow, modifier = modifier) { Column(content = content) }
    }
}

/** Поле ввода по стилю: M3 outlined; у остальных — залитое поле без линии. */
@Composable
fun KitTextField(
    value: String,
    onChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    singleLine: Boolean = false,
    minLines: Int = 1,
    maxLines: Int = if (singleLine) 1 else 6,
    label: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val st = LocalVgdStyle.current
    if (st.kind == Kind.YOU) {
        OutlinedTextField(
            value, onChange, modifier.fillMaxWidth(),
            label = label?.let { { Text(it) } },
            placeholder = { Text(placeholder) },
            singleLine = singleLine, minLines = minLines, maxLines = maxLines,
            shape = RoundedCornerShape(16.dp),
            trailingIcon = trailing,
            colors = OutlinedTextFieldDefaults.colors(unfocusedContainerColor = groupColor(), focusedContainerColor = groupColor()),
        )
        return
    }
    val radius = when (st.kind) { Kind.IOS -> 12.dp; Kind.ONEUI -> 22.dp; Kind.HYPER -> 16.dp; else -> 20.dp }
    val bg = if (st.kind == Kind.IOS || st.kind == Kind.HARMONY) groupColor() else fieldColor()
    TextField(
        value, onChange, modifier.fillMaxWidth(),
        placeholder = { Text(label ?: placeholder) },
        singleLine = singleLine, minLines = minLines, maxLines = maxLines,
        shape = RoundedCornerShape(radius),
        trailingIcon = trailing,
        colors = TextFieldDefaults.colors(
            focusedContainerColor = bg, unfocusedContainerColor = bg, disabledContainerColor = bg,
            focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent, disabledIndicatorColor = Color.Transparent,
        ),
    )
}

/** Пустое состояние: значок в круге, заголовок, подсказка. */
@Composable
fun EmptyState(icon: ImageVector, title: String, body: String, action: (@Composable () -> Unit)? = null) {
    val c = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(88.dp).clip(CircleShape).background(c.primaryContainer), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = c.onPrimaryContainer, modifier = Modifier.size(42.dp))
        }
        Spacer(Modifier.height(16.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Spacer(Modifier.height(6.dp))
        Text(body, style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant, textAlign = TextAlign.Center)
        if (action != null) {
            Spacer(Modifier.height(12.dp))
            action()
        }
    }
}

/** Заметка-плашка (нужно разрешение, ошибка подготовки). */
@Composable
fun Notice(text: String, button: String?, onClick: () -> Unit) {
    val st = LocalVgdStyle.current
    val c = MaterialTheme.colorScheme
    KitCard(Modifier.fillMaxWidth().padding(horizontal = st.hPad.coerceAtLeast(12.dp)), color = c.secondaryContainer) {
        Column(Modifier.padding(16.dp)) {
            Text(text, style = MaterialTheme.typography.bodyMedium, color = c.onSecondaryContainer)
            if (button != null) {
                Spacer(Modifier.height(10.dp))
                KitButton(button, onClick, kind = Btn.PRIMARY)
            }
        }
    }
}

/** Пустая карточка-разделитель высотой с отступ (для списков). */
@Composable
fun Gap(h: Dp = 8.dp) = Spacer(Modifier.height(h))

/** Ничего не делать (для превью). */
internal val noop: () -> Unit = {}

/** Запомнить лямбду без пересоздания. */
@Composable
internal fun <T> rememberValue(v: T): T = remember { v }
