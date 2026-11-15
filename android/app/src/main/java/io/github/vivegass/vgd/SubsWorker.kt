package io.github.vivegass.vgd

import android.content.Context
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.delay
import java.util.concurrent.TimeUnit

/**
 * Подписки по расписанию — та же логика, что `vgd sync` на ПК: раз в 6 часов, только по Wi-Fi и при
 * нормальном заряде. Работа идёт как служба переднего плана (уведомление с прогрессом), пока очередь не опустеет.
 */
class SubsWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun getForegroundInfo(): ForegroundInfo {
        val n = Notifications.progress(applicationContext, Vgd.jobs.value)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ForegroundInfo(NOTIFICATION_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(NOTIFICATION_ID, n)
        }
    }

    override suspend fun doWork(): Result {
        return try {
            setForeground(getForegroundInfo())
            val core = Vgd.core()
            val n = core.checkSubscriptions(null)
            Vgd.addLog("Подписки по расписанию: проверяю $n")
            // ждём, пока очередь опустеет (WorkManager сам остановит, если система попросит)
            while (core.active()) {
                delay(5_000)
                setForeground(getForegroundInfo())
            }
            Result.success()
        } catch (e: Throwable) {
            Vgd.addLog("Ошибка: подписки по расписанию: ${e.text()}")
            Result.retry()
        }
    }

    companion object {
        private const val NAME = "subscriptions"
        private const val NOTIFICATION_ID = 2

        fun enabled(ctx: Context) = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).getBoolean("subs_auto", false)

        /** Включить / выключить проверку раз в 6 часов. */
        fun schedule(ctx: Context, on: Boolean) {
            ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).edit().putBoolean("subs_auto", on).apply()
            val wm = WorkManager.getInstance(ctx)
            if (!on) {
                wm.cancelUniqueWork(NAME)
                return
            }
            val req = PeriodicWorkRequestBuilder<SubsWorker>(6, TimeUnit.HOURS)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.UNMETERED)
                        .setRequiresBatteryNotLow(true)
                        .build(),
                )
                .build()
            wm.enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.UPDATE, req)
        }
    }
}
