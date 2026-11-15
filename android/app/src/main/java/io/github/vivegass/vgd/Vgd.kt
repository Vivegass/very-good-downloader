package io.github.vivegass.vgd

import android.content.Context
import android.media.MediaScannerConnection
import android.os.Environment
import android.util.Log
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import io.github.vivegass.vgd.core.Core
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState
import io.github.vivegass.vgd.core.QueueListener
import io.github.vivegass.vgd.core.ToolPaths
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import java.io.File

/**
 * Ядро программы на телефоне: одно на процесс.
 *
 * - распаковывает Python + yt-dlp и ffmpeg (youtubedl-android) и передаёт пути в Rust (`init`);
 * - держит очередь загрузок (`Core` из libvgd.so) и отдаёт её экрану и службе через StateFlow;
 * - после каждой загрузки — MediaScanner (файлы видны в Галерее и Файлах) и уведомление «скачано».
 */
object Vgd {
    private const val TAG = "Vgd"

    private val lock = Any()
    @Volatile private var core: Core? = null
    private lateinit var app: Context

    private val _jobs = MutableStateFlow<List<JobInfo>>(emptyList())
    val jobs: StateFlow<List<JobInfo>> = _jobs

    private val _log = MutableStateFlow<List<String>>(emptyList())
    val log: StateFlow<List<String>> = _log

    /** null — ещё не готово; "" — готово; иначе текст ошибки распаковки. */
    private val _ready = MutableStateFlow<String?>(null)
    val ready: StateFlow<String?> = _ready

    fun attach(context: Context) {
        app = context.applicationContext
    }

    /** Папка загрузок по умолчанию: Download/Very Good Downloader. */
    fun defaultDownloadDir(): File =
        File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Very Good Downloader")

    /**
     * Подготовить инструменты и ядро (первый раз — распаковка Python, несколько секунд).
     * Вызывать не из главной нити.
     */
    fun core(): Core {
        core?.let { return it }
        synchronized(lock) {
            core?.let { return it }
            try {
                val paths = unpackTools(app)
                io.github.vivegass.vgd.core.init(paths)
                // только debug: адрес mock-сервера релизов (files/update_url.txt) вместо GitHub
                if (BuildConfig.DEBUG) {
                    File(app.filesDir, "update_url.txt").takeIf { it.isFile }?.readText()?.trim()?.takeIf { it.isNotEmpty() }?.let {
                        io.github.vivegass.vgd.core.setUpdateUrl(it)
                    }
                }
                io.github.vivegass.vgd.core.setVkSearcher(VkAgent)
                io.github.vivegass.vgd.core.setSpotifyPage(SpotifyAgent)
                // yt-dlp, вшитый в APK, стареет (2025.11 уже не качает SoundCloud): обновить до первой загрузки,
                // иначе первая же ссылка из «Поделиться» пошла бы старым
                updateYtdlpIfOld()
                val c = Core(listener)
                core = c
                _ready.value = ""
                addLog("Very Good Downloader для Android ${BuildConfig.VERSION_NAME} (ядро ${io.github.vivegass.vgd.core.version()})")
                Thread { Updater.checkIfDue(app) }.start()
                return c
            } catch (e: Throwable) {
                Log.e(TAG, "init", e)
                _ready.value = e.message ?: e.toString()
                throw e
            }
        }
    }

    fun coreOrNull(): Core? = core

    /**
     * yt-dlp из youtubedl-android вшит в APK и стареет; YouTube ломает старые версии. Старше 45 дней и очередь
     * пуста — обновить сразу при запуске (как кнопка в настройках).
     */
    private fun updateYtdlpIfOld() {
        try {
            val v = io.github.vivegass.vgd.core.ytdlpVersion() ?: return
            val m = Regex("""(\d{4})\.(\d{1,2})\.(\d{1,2})""").find(v) ?: return
            val (y, mo, d) = m.destructured
            val built = java.time.LocalDate.of(y.toInt(), mo.toInt(), d.toInt())
            val age = java.time.temporal.ChronoUnit.DAYS.between(built, java.time.LocalDate.now())
            if (age < 45) return
            addLog("yt-dlp $v старше 45 дней — обновляю…")
            val nv = io.github.vivegass.vgd.core.updateYtdlp()
            addLog("yt-dlp обновлён: $nv")
        } catch (e: Throwable) {
            addLog("Внимание: не удалось обновить yt-dlp: ${e.text()}")
        }
    }

    private fun unpackTools(ctx: Context): ToolPaths {
        YoutubeDL.getInstance().init(ctx)
        FFmpeg.getInstance().init(ctx)
        val nativeDir = ctx.applicationInfo.nativeLibraryDir
        val base = File(ctx.noBackupFilesDir, "youtubedl-android")
        val packages = File(base, "packages")
        val py = File(packages, "python")
        val ff = File(packages, "ffmpeg")
        val aria = File(packages, "aria2c")
        val qjs = File(nativeDir, "libqjs.so")
        val env = hashMapOf(
            "LD_LIBRARY_PATH" to "${py.absolutePath}/usr/lib:${ff.absolutePath}/usr/lib:${aria.absolutePath}/usr/lib",
            "SSL_CERT_FILE" to "${py.absolutePath}/usr/etc/tls/cert.pem",
            "PYTHONHOME" to "${py.absolutePath}/usr",
            "HOME" to "${py.absolutePath}/usr",
            "TMPDIR" to ctx.cacheDir.absolutePath,
            "PATH" to (System.getenv("PATH") ?: "/system/bin") + ":" + nativeDir,
        )
        return ToolPaths(
            dataDir = File(ctx.filesDir, "vgd").absolutePath,
            downloadDir = defaultDownloadDir().absolutePath,
            python = File(nativeDir, "libpython.so").absolutePath,
            ytdlp = File(base, "yt-dlp/yt-dlp").absolutePath,
            ffmpeg = File(nativeDir, "libffmpeg.so").absolutePath,
            quickjs = if (qjs.exists()) qjs.absolutePath else "",
            env = env,
            minFreeGb = 1.0,
        )
    }

    /** Поставить ссылку в очередь и запустить фоновую службу. Можно звать из главной нити. */
    fun add(context: Context, url: String, audioOnly: Boolean = false) {
        DownloadService.start(context)
        Thread {
            try {
                core().add(url, audioOnly)
            } catch (e: Throwable) {
                addLog("Ошибка: $e")
            }
        }.start()
    }

    /** С параметрами ссылки (JSON JobOpts: from, to, audio_only, split_tracks, preset). */
    fun addWith(context: Context, url: String, optsJson: String) {
        DownloadService.start(context)
        Thread {
            try {
                core().addWith(url, optsJson)
            } catch (e: Throwable) {
                addLog("Ошибка: $e")
            }
        }.start()
    }

    /** Строка в журнал (из любой нити: `update` не теряет строки при одновременной записи). */
    fun addLog(line: String) {
        _log.update { l -> if (l.size >= 3000) l.drop(l.size - 2999) + line else l + line }
    }

    private val listener = object : QueueListener {
        override fun onJobs(jobs: List<JobInfo>) {
            _jobs.value = jobs
        }

        override fun onLog(line: String) = addLog(line)

        override fun onFinished(job: JobInfo) {
            if (job.state == JobState.DONE && job.files.isNotEmpty()) {
                MediaScannerConnection.scanFile(app, job.files.toTypedArray(), null, null)
            }
            Notifications.finished(app, job)
        }

        override fun onIdle(active: Boolean) {
            if (!active) DownloadService.idle(app)
        }
    }
}

/** Текст ошибки для журнала и экрана: у ошибок ядра — сама причина (без служебного «reason=»). */
fun Throwable.text(): String = (this as? io.github.vivegass.vgd.core.VgdException.Failed)?.reason ?: message ?: toString()
