package io.github.vivegass.vgd

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import io.github.vivegass.vgd.core.UpdateInfo
import io.github.vivegass.vgd.core.UpdateProgress
import io.github.vivegass.vgd.core.updateCheck
import io.github.vivegass.vgd.core.updateDownload
import io.github.vivegass.vgd.core.updateDue
import io.github.vivegass.vgd.core.updatePending
import io.github.vivegass.vgd.core.updatePrefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Обновление приложения через GitHub Releases (общий с ПК релиз `v…`, ассет `…-android-<версия>-arm64.apk`) — та же логика,
 * что у ПК (`core/src/update.rs`): проверка, скачивание с докачкой и проверкой SHA-256 — в ядре;
 * здесь — плашка, уведомление с прогрессом и установка через PackageInstaller.
 * Встаёт поверх только APK с тем же ключом подписи (свой release-ключ — до первого публичного APK).
 */
object Updater {
    sealed interface State {
        data object Idle : State
        data object Checking : State
        /** проверили — новее нет */
        data object Latest : State
        data class Found(val info: UpdateInfo) : State
        data class Downloading(val info: UpdateInfo, val done: Long, val total: Long) : State
        data class Installing(val info: UpdateInfo) : State
        /** нужно разрешить «Установку неизвестных приложений» — вернётся в приложение и поставит */
        data class NeedPermission(val info: UpdateInfo, val file: File) : State
        data class Error(val message: String, val info: UpdateInfo?) : State
    }

    private val _state = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = _state

    private const val NOTIFICATION_ID = 3
    private const val CH = "progress"
    private const val ACTION_INSTALLED = "io.github.vivegass.vgd.UPDATE_INSTALLED"

    fun current() = BuildConfig.VERSION_NAME

    /** Найденное раньше (без сети) — сразу после запуска. */
    fun restore() {
        if (_state.value != State.Idle) return
        runCatching { updatePending(current()) }.getOrNull()?.let { _state.value = State.Found(it) }
    }

    /** Проверка при запуске — если включено «автоматически» и прошло 6 ч. */
    fun checkIfDue(ctx: Context) {
        restore()
        if (runCatching { updateDue() }.getOrDefault(false)) check(ctx, manual = false)
    }

    /** Проверить (в своей нити). manual — показать «последняя версия» / ошибку. */
    fun check(ctx: Context, manual: Boolean) {
        if (_state.value is State.Downloading || _state.value is State.Installing) return
        if (manual) _state.value = State.Checking
        Thread {
            try {
                val found = updateCheck(current())
                val skipped = found != null && !manual && runCatching { updatePrefs().skip }.getOrDefault("") == found.version
                _state.value = when {
                    found == null -> if (manual) State.Latest else State.Idle
                    skipped -> State.Idle
                    else -> State.Found(found)
                }
                if (found != null && !skipped) Vgd.addLog("Доступна версия ${found.version} (сейчас ${current()})")
            } catch (e: Throwable) {
                if (manual) _state.value = State.Error("Не удалось проверить: ${e.text()}", null)
                Vgd.addLog("Внимание: проверка обновлений: ${e.text()}")
            }
        }.start()
    }

    /** «Позже» — спрятать плашку до следующего запуска. */
    fun dismiss() {
        _state.value = State.Idle
    }

    /** Скачать (уведомление с прогрессом), проверить SHA-256 и поставить. */
    fun download(ctx: Context, info: UpdateInfo) {
        val app = ctx.applicationContext
        if (_state.value is State.Downloading) return
        _state.value = State.Downloading(info, 0, info.size.toLong())
        Thread {
            val nm = Notifications.manager(app)
            var lastShown = 0L
            try {
                val dir = File(app.cacheDir, "update").apply { mkdirs() }
                val path = updateDownload(info, dir.absolutePath, object : UpdateProgress {
                    override fun onProgress(done: ULong, total: ULong): Boolean {
                        val d = done.toLong()
                        val t = total.toLong()
                        _state.value = State.Downloading(info, d, t)
                        val now = System.currentTimeMillis()
                        if (now - lastShown > 500) {
                            lastShown = now
                            nm.notify(NOTIFICATION_ID, progressNotification(app, info, d, t))
                        }
                        return _state.value is State.Downloading
                    }
                })
                nm.cancel(NOTIFICATION_ID)
                Vgd.addLog("Обновление ${info.version} скачано, контрольная сумма совпала")
                install(app, info, File(path))
            } catch (e: Throwable) {
                nm.cancel(NOTIFICATION_ID)
                _state.value = State.Error("Не скачалось: ${e.text()}", info)
                Vgd.addLog("Ошибка: обновление ${info.version}: ${e.text()}")
            }
        }.start()
    }

    private fun progressNotification(ctx: Context, info: UpdateInfo, done: Long, total: Long) =
        NotificationCompat.Builder(ctx, CH)
            .setSmallIcon(R.drawable.ic_stat_download)
            .setContentTitle("Обновление ${info.version}")
            .setContentText(if (total > 0) "${done * 100 / total}% · ${done / 1_048_576} из ${total / 1_048_576} МБ" else "скачиваю…")
            .setProgress(if (total > 0) 100 else 0, if (total > 0) (done * 100 / total).toInt() else 0, total <= 0)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()

    /** Разрешено ли ставить приложения (Android 8+: «Установка неизвестных приложений» для этого приложения). */
    fun canInstall(ctx: Context) = ctx.packageManager.canRequestPackageInstalls()

    /** Поставить скачанный APK. Без разрешения — открыть его настройку; вернувшись, `resumeInstall`. */
    fun install(ctx: Context, info: UpdateInfo, file: File) {
        val app = ctx.applicationContext
        if (!canInstall(app)) {
            _state.value = State.NeedPermission(info, file)
            val i = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + app.packageName))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching { app.startActivity(i) }
            return
        }
        _state.value = State.Installing(info)
        try {
            val pi = app.packageManager.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            params.setAppPackageName(app.packageName)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
            }
            val id = pi.createSession(params)
            pi.openSession(id).use { s ->
                s.openWrite("base.apk", 0, file.length()).use { out ->
                    file.inputStream().use { it.copyTo(out) }
                    s.fsync(out)
                }
                val intent = Intent(app, InstallReceiver::class.java).setAction(ACTION_INSTALLED)
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                s.commit(PendingIntent.getBroadcast(app, id, intent, flags).intentSender)
            }
            Vgd.addLog("Ставлю обновление ${info.version}…")
        } catch (e: Throwable) {
            _state.value = State.Error("Не установилось: ${e.text()}", info)
            Vgd.addLog("Ошибка: установка обновления: ${e.text()}")
        }
    }

    /** Вернулись из настройки «Установка неизвестных приложений». */
    fun resumeInstall(ctx: Context) {
        val s = _state.value
        if (s is State.NeedPermission && canInstall(ctx)) install(ctx, s.info, s.file)
    }

    internal fun installResult(ctx: Context, intent: Intent) {
        when (val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            // система спрашивает подтверждение — показать её окно
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT) ?: return
                ctx.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
            PackageInstaller.STATUS_SUCCESS -> Vgd.addLog("Обновление установлено")
            else -> {
                val msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "код $status"
                val info = (_state.value as? State.Installing)?.info
                _state.value = State.Error(
                    if (status == PackageInstaller.STATUS_FAILURE_CONFLICT || status == PackageInstaller.STATUS_FAILURE_INCOMPATIBLE)
                        "Не установилось: APK подписан другим ключом — удалите старую версию и поставьте новую вручную"
                    else "Не установилось: $msg",
                    info,
                )
                Vgd.addLog("Ошибка: установка обновления: $msg")
            }
        }
    }

    // ---------- проверка раз в 6 часов ----------

    fun schedule(ctx: Context, on: Boolean) {
        val wm = WorkManager.getInstance(ctx)
        if (!on) {
            wm.cancelUniqueWork("update")
            return
        }
        val req = PeriodicWorkRequestBuilder<UpdateWorker>(6, TimeUnit.HOURS)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        wm.enqueueUniquePeriodicWork("update", ExistingPeriodicWorkPolicy.KEEP, req)
    }

    /** Уведомление «Доступна версия X» (из фоновой проверки). */
    internal fun notifyFound(ctx: Context, info: UpdateInfo) {
        val open = PendingIntent.getActivity(
            ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(ctx, "done")
            .setSmallIcon(R.drawable.ic_stat_download)
            .setContentTitle("Доступна версия ${info.version}")
            .setContentText("Very Good Downloader — нажмите, чтобы обновить")
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        runCatching { Notifications.manager(ctx).notify(NOTIFICATION_ID + 1, n) }
    }
}

class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) = Updater.installResult(ctx, intent)
}

/** Фоновая проверка обновлений (раз в 6 ч, если включено «проверять автоматически»). */
class UpdateWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result {
        return try {
            Vgd.core()
            if (!updatePrefs().auto) return Result.success()
            val found = updateCheck(Updater.current())
            if (found != null && updatePrefs().skip != found.version) Updater.notifyFound(applicationContext, found)
            Result.success()
        } catch (_: Throwable) {
            Result.retry()
        }
    }
}
