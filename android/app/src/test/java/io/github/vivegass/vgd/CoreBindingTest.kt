package io.github.vivegass.vgd

import io.github.vivegass.vgd.core.Core
import io.github.vivegass.vgd.core.JobInfo
import io.github.vivegass.vgd.core.JobState
import io.github.vivegass.vgd.core.QueueListener
import io.github.vivegass.vgd.core.ToolPaths
import io.github.vivegass.vgd.core.extractUrl
import io.github.vivegass.vgd.core.init
import io.github.vivegass.vgd.core.saveSettingsJson
import io.github.vivegass.vgd.core.settingsJson
import io.github.vivegass.vgd.core.themeList
import io.github.vivegass.vgd.core.themeSchemeJson
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Связка Kotlin ↔ Rust через UniFFI на ПК (libvgd собрана под Windows, JNA): те же вызовы, что делает APK.
 * Сам yt-dlp здесь не запускается — «python» указывает на несуществующий файл, загрузка падает с ошибкой.
 */
class CoreBindingTest {
    private val tmp = File(System.getProperty("java.io.tmpdir"), "vgd-binding-test-${System.nanoTime()}")

    private fun paths() = ToolPaths(
        dataDir = File(tmp, "data").absolutePath,
        downloadDir = File(tmp, "dl").absolutePath,
        python = File(tmp, "нет-python").absolutePath,
        ytdlp = File(tmp, "yt-dlp").absolutePath,
        ffmpeg = File(tmp, "ffmpeg").absolutePath,
        quickjs = "",
        env = mapOf("PYTHONHOME" to tmp.absolutePath),
        minFreeGb = 0.0,
    )

    @Test
    fun everythingThroughUniffi() {
        init(paths())
        // ссылка из текста «Поделиться»
        assertEquals("https://youtu.be/abc", extractUrl("Смотри https://youtu.be/abc"))
        // первый запуск: настройки телефона, папка по умолчанию — из init
        val s = JSONObject(settingsJson())
        assertEquals("none", s.getString("cookies"))
        assertEquals(File(tmp, "dl").absolutePath, s.getString("out_dir"))
        s.put("max_height", 1080)
        saveSettingsJson(s.toString())
        assertEquals(1080, JSONObject(settingsJson()).getInt("max_height"))
        // темы с ПК
        assertTrue(themeList().any { it.key == "material" })
        assertTrue(JSONObject(themeSchemeJson("material", true)).getString("primary").startsWith("#"))

        // очередь: добавить → загрузка падает (нет python) → on_finished(ERROR), on_idle
        val finished = CountDownLatch(1)
        val idle = CountDownLatch(1)
        var last: JobInfo? = null
        val logs = mutableListOf<String>()
        val core = Core(object : QueueListener {
            override fun onJobs(jobs: List<JobInfo>) {}
            override fun onLog(line: String) { synchronized(logs) { logs += line } }
            override fun onFinished(job: JobInfo) { last = job; finished.countDown() }
            override fun onIdle(active: Boolean) { if (!active) idle.countDown() }
        })
        val id = core.add("https://www.youtube.com/watch?v=jNQXAC9IVRw", false)
        assertTrue("загрузка не закончилась за 90 с", finished.await(90, TimeUnit.SECONDS))
        assertNotNull(last)
        assertEquals(id, last!!.id)
        assertEquals(JobState.ERROR, last!!.state)
        assertTrue(idle.await(30, TimeUnit.SECONDS))
        assertTrue(!core.active())
        core.retry(id)
        core.pause(id)
        core.clearDone()
        assertTrue(core.jobs().size <= 1)
        tmp.deleteRecursively()
    }
}
