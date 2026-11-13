package io.github.vivegass.vgd

import android.annotation.SuppressLint
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.content.ContextCompat
import io.github.vivegass.vgd.core.JobState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

/**
 * Служба переднего плана (тип dataSync): пока что-то качается — уведомление с прогрессом,
 * WakeLock + WifiLock. Очередь опустела — служба уходит сама.
 *
 * Android 15: у dataSync лимит 6 ч в сутки; по `onTimeout` очередь встаёт на паузу
 * (докачается кнопкой «продолжить» — скачанное пропустится по архиву, `.part` yt-dlp продолжит).
 */
class DownloadService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var wake: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null
    private var foreground = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Notifications.ensureChannels(this)
        goForeground()
        acquireLocks()
        scope.launch {
            Vgd.jobs.collectLatest { jobs ->
                val n = Notifications.progress(this@DownloadService, jobs)
                Notifications.manager(this@DownloadService).notify(Notifications.PROGRESS_ID, n)
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        goForeground()
        when (intent?.action) {
            ACTION_STOP_IF_IDLE -> stopIfIdle()
            ACTION_PAUSE_ALL -> Thread { Vgd.coreOrNull()?.pauseAll() }.start()
        }
        // Ядро поднимаем здесь же: служба могла быть перезапущена системой.
        Thread {
            try {
                Vgd.core()
            } catch (_: Throwable) {
                stopSelf()
            }
        }.start()
        return START_NOT_STICKY
    }

    private fun goForeground() {
        if (foreground) return
        val n = Notifications.progress(this, Vgd.jobs.value)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(Notifications.PROGRESS_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(Notifications.PROGRESS_ID, n)
        }
        foreground = true
    }

    private fun stopIfIdle() {
        val active = Vgd.coreOrNull()?.active()
            ?: Vgd.jobs.value.any { it.state == JobState.QUEUED || it.state == JobState.RUNNING }
        if (!active) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    /** Android 15+: лимит фоновой работы dataSync исчерпан — всё на паузу и выйти. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        Vgd.addLog("Android остановил фоновую загрузку (лимит 6 ч в сутки) — очередь на паузе.")
        Thread { Vgd.coreOrNull()?.pauseAll() }.start()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    @SuppressLint("WakelockTimeout")
    private fun acquireLocks() {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vgd:download").apply { acquire() }
        val wm = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        @Suppress("DEPRECATION")
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
        wifi = wm.createWifiLock(mode, "vgd:download").apply { acquire() }
    }

    override fun onDestroy() {
        scope.cancel()
        wake?.takeIf { it.isHeld }?.release()
        wifi?.takeIf { it.isHeld }?.release()
        super.onDestroy()
    }

    companion object {
        const val ACTION_STOP_IF_IDLE = "io.github.vivegass.vgd.STOP_IF_IDLE"
        const val ACTION_PAUSE_ALL = "io.github.vivegass.vgd.PAUSE_ALL"

        /** Запустить службу (из Activity, плитки или ShareActivity — приложение на экране). */
        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, DownloadService::class.java))
        }

        /** Очередь опустела — служба проверит и уйдёт. */
        fun idle(context: Context) {
            try {
                context.startService(Intent(context, DownloadService::class.java).setAction(ACTION_STOP_IF_IDLE))
            } catch (_: Exception) {
                // службы уже нет / приложение в фоне — нечего останавливать
            }
        }
    }
}
