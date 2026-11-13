package io.github.vivegass.vgd

import android.app.Application

class VgdApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Vgd.attach(this)
        Notifications.ensureChannels(this)
        Shortcuts.publish(this)
        // Распаковка Python/ffmpeg (первый запуск — несколько секунд) — сразу в фоне,
        // чтобы к первой ссылке всё было готово.
        Thread {
            try {
                Vgd.core()
            } catch (_: Throwable) {
                // ошибку покажет экран (Vgd.ready)
            }
        }.start()
    }
}
