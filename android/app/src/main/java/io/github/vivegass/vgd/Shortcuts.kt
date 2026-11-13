package io.github.vivegass.vgd

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat

/**
 * Ярлыки «Видео» и «Звук»:
 * - в системной панели «Поделиться» — цели Direct Share (верхний ряд, рядом с контактами);
 * - долгое нажатие на значок приложения — скачать ссылку из буфера обмена.
 */
object Shortcuts {
    const val VIDEO = "video"
    const val AUDIO = "audio"
    private const val CATEGORY = "io.github.vivegass.vgd.category.DOWNLOAD"

    fun publish(ctx: Context) {
        val items = listOf(
            Triple(VIDEO, R.string.sc_video_short to R.string.sc_video_long, R.mipmap.ic_launcher),
            Triple(AUDIO, R.string.sc_audio_short to R.string.sc_audio_long, R.mipmap.ic_share_audio),
        )
        items.forEachIndexed { rank, (id, labels, icon) ->
            try {
                val info = ShortcutInfoCompat.Builder(ctx, id)
                    .setShortLabel(ctx.getString(labels.first))
                    .setLongLabel(ctx.getString(labels.second))
                    .setIcon(IconCompat.createWithResource(ctx, icon))
                    // с рабочего стола — ссылка из буфера; из «Поделиться» система откроет ShareActivity с текстом
                    .setIntent(
                        Intent(ctx, ClipActivity::class.java)
                            .setAction(Intent.ACTION_VIEW)
                            .putExtra(ClipActivity.EXTRA_AUDIO, id == AUDIO),
                    )
                    .setCategories(setOf(CATEGORY))
                    .setLongLived(true)
                    .setRank(rank)
                    .build()
                ShortcutManagerCompat.pushDynamicShortcut(ctx, info)
            } catch (_: Exception) {
                // лимит ярлыков / лаунчер без ярлыков — не страшно
            }
        }
    }

    /** Чем чаще используется, тем выше система ставит ярлык. */
    fun used(ctx: Context, id: String) {
        try {
            ShortcutManagerCompat.reportShortcutUsed(ctx, id)
        } catch (_: Exception) {
        }
    }
}
