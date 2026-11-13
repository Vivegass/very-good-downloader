package io.github.vivegass.vgd

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState

/** Уведомления: прогресс фоновой службы и «скачано / не скачалось». Цвет — из системы (Material You). */
object Notifications {
    const val PROGRESS_ID = 1
    private const val CH_PROGRESS = "progress"
    private const val CH_DONE = "done"

    fun manager(ctx: Context) = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    fun ensureChannels(ctx: Context) {
        val m = manager(ctx)
        m.createNotificationChannel(
            NotificationChannel(CH_PROGRESS, ctx.getString(R.string.ch_progress), NotificationManager.IMPORTANCE_LOW).apply {
                setShowBadge(false)
            },
        )
        m.createNotificationChannel(
            NotificationChannel(CH_DONE, ctx.getString(R.string.ch_done), NotificationManager.IMPORTANCE_DEFAULT),
        )
    }

    private fun openApp(ctx: Context): PendingIntent =
        PendingIntent.getActivity(
            ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    /** Уведомление службы: что качается, сколько ждёт. */
    fun progress(ctx: Context, jobs: List<JobInfo>): Notification {
        val running = jobs.firstOrNull { it.state == JobState.RUNNING }
        val waiting = jobs.count { it.state == JobState.QUEUED }
        val b = NotificationCompat.Builder(ctx, CH_PROGRESS)
            .setSmallIcon(R.drawable.ic_stat_download)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setContentIntent(openApp(ctx))
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        if (running != null) {
            b.setContentTitle(running.title.ifEmpty { running.url })
            val parts = listOfNotNull(
                running.item.takeIf { it.isNotEmpty() },
                running.message.takeIf { it.isNotEmpty() } ?: "${running.percent.toInt()}%",
                running.speed.takeIf { it.isNotEmpty() },
                if (waiting > 0) "ещё $waiting в очереди" else null,
            )
            b.setContentText(parts.joinToString(" · "))
            if (running.percent > 0f) b.setProgress(100, running.percent.toInt(), false) else b.setProgress(0, 0, true)
        } else {
            b.setContentTitle(ctx.getString(R.string.app_name))
            b.setContentText(if (waiting > 0) "в очереди: $waiting" else "готовлюсь…")
            b.setProgress(0, 0, true)
        }
        b.addAction(
            0, "пауза",
            PendingIntent.getService(
                ctx, 1, Intent(ctx, DownloadService::class.java).setAction(DownloadService.ACTION_PAUSE_ALL),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            ),
        )
        return b.build()
    }

    /** «Скачано» / «Не скачалось». */
    fun finished(ctx: Context, job: JobInfo) {
        if (job.state != JobState.DONE && job.state != JobState.ERROR) return
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        val text = if (job.state == JobState.DONE) {
            if (job.files.isEmpty()) "уже было скачано раньше" else "файлов: ${job.files.size}"
        } else {
            job.error.ifEmpty { "ошибка" }
        }
        val n = NotificationCompat.Builder(ctx, CH_DONE)
            .setSmallIcon(R.drawable.ic_stat_download)
            .setContentTitle((if (job.state == JobState.DONE) "Скачано: " else "Не скачалось: ") + job.title.ifEmpty { job.url })
            .setContentText(text)
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx))
            .build()
        manager(ctx).notify(1000 + (job.id % 100000uL).toInt(), n)
    }
}
