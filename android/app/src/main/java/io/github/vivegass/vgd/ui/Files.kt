package io.github.vivegass.vgd.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.os.Environment
import android.provider.DocumentsContract
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.File

/**
 * Открыть скачанное: файл — в приложении по выбору системы (`content://` через FileProvider, свой MIME),
 * папку — в системных «Файлах» (DocumentsUI).
 */
object Files {
    private val MIME = mapOf(
        "mkv" to "video/x-matroska", "mp4" to "video/mp4", "webm" to "video/webm", "mov" to "video/quicktime",
        "opus" to "audio/ogg", "ogg" to "audio/ogg", "m4a" to "audio/mp4", "mp3" to "audio/mpeg", "flac" to "audio/flac",
        "wav" to "audio/wav", "jpg" to "image/jpeg", "png" to "image/png", "webp" to "image/webp", "lrc" to "text/plain",
        "txt" to "text/plain", "srt" to "text/plain", "vtt" to "text/vtt",
    )

    fun mime(f: File): String {
        val ext = f.extension.lowercase()
        return MIME[ext] ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
    }

    /** Главный файл загрузки: видео / звук, а не обложка или «_откуда скачано.txt». */
    fun main(files: List<String>): File? {
        val fs = files.map { File(it) }
        return fs.firstOrNull { mime(it).startsWith("video/") || mime(it).startsWith("audio/") } ?: fs.firstOrNull()
    }

    /** Папка загрузки: у одного файла — его папка, у плейлиста — общая папка треков. */
    fun folder(files: List<String>): File? {
        val dirs = files.mapNotNull { File(it).parentFile }.distinct()
        if (dirs.isEmpty()) return null
        var common: File? = dirs.first()
        while (common != null && dirs.any { !it.absolutePath.startsWith(common!!.absolutePath) }) common = common.parentFile
        return common
    }

    /** false — файла нет (перемещён или удалён). */
    fun openFile(ctx: Context, f: File): Boolean {
        if (!f.isFile) return false
        val uri = FileProvider.getUriForFile(ctx, ctx.packageName + ".files", f)
        val view = Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime(f)).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        return try {
            ctx.startActivity(Intent.createChooser(view, f.name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            true
        } catch (_: ActivityNotFoundException) {
            false
        }
    }

    /** Папка в «Файлах»: documentId `primary:Download/…`. false — папки нет. */
    fun openFolder(ctx: Context, dir: File): Boolean {
        if (!dir.isDirectory) return false
        val root = Environment.getExternalStorageDirectory().absolutePath
        val rel = dir.absolutePath.removePrefix(root).trimStart('/')
        val docId = "primary:$rel"
        val auth = "com.android.externalstorage.documents"
        val tries = listOf(
            Intent(Intent.ACTION_VIEW).setDataAndType(DocumentsContract.buildDocumentUri(auth, docId), DocumentsContract.Document.MIME_TYPE_DIR),
            Intent(Intent.ACTION_VIEW).setDataAndType(DocumentsContract.buildTreeDocumentUri(auth, docId), DocumentsContract.Document.MIME_TYPE_DIR),
            Intent(Intent.ACTION_VIEW).setDataAndType(DocumentsContract.buildRootUri(auth, "primary"), DocumentsContract.Root.MIME_TYPE_ITEM),
        )
        for (i in tries) {
            try {
                ctx.startActivity(i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION))
                return true
            } catch (_: Exception) {
            }
        }
        return false
    }
}
