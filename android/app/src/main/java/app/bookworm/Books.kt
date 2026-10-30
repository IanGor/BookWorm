package app.bookworm

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File

/**
 * Книги, шрифты и место чтения во внутреннем хранилище приложения.
 * Книга — копия файла в files/books.
 */
class Books(private val ctx: Context) {
    val dir = File(ctx.filesDir, "books")
    val fontDir = File(ctx.filesDir, "fonts")
    private val prefs = ctx.getSharedPreferences("reader", Context.MODE_PRIVATE)

    /** Шрифты и тестовый корпус из APK — на диск, откуда их читает ядро. */
    fun prepare() {
        dir.mkdirs()
        fontDir.mkdirs()
        val fonts = ctx.assets.list("fonts").orEmpty().toSet()
        for (name in fonts) {
            val f = File(fontDir, name)
            val size = ctx.assets.openFd("fonts/$name").use { it.length }
            if (f.length() != size) copyAsset("fonts/$name", f)
        }
        // Шрифты, которых больше нет во встроенном наборе (после обновления), убираем.
        fontDir.listFiles()?.filter { it.name !in fonts }?.forEach { it.delete() }
        // Остатки прежней версии: правок больше нет.
        dir.listFiles { f -> f.name.endsWith(".edits") }?.forEach { it.delete() }
    }

    private fun copyAsset(path: String, to: File) {
        ctx.assets.open(path).use { input -> writeSafely(to) { input.copyTo(it) } }
    }

    /** Книги, последние открытые — первыми. */
    fun list(): List<File> =
        dir.listFiles { f -> f.isFile && isBook(f.name) }.orEmpty()
            .sortedWith(compareByDescending<File> { prefs.getLong("opened:${it.name}", 0) }.thenBy { it.name })

    /** Копирует книгу из другого приложения к себе; повторный импорт заменяет файл, правки остаются. */
    fun import(uri: Uri): File {
        val name = displayName(uri).replace(Regex("[/\\\\:]"), "_").ifBlank { "книга.fb2" }
        val to = File(dir, name)
        val input = ctx.contentResolver.openInputStream(uri) ?: error("файл не открывается")
        input.use { i -> writeSafely(to) { i.copyTo(it) } }
        return to
    }

    private fun displayName(uri: Uri): String {
        if (uri.scheme == "content") {
            ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0)?.let { return it }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: ""
    }

    var last: File?
        get() = prefs.getString("last", null)?.let { File(dir, it) }?.takeIf { it.exists() }
        set(value) {
            prefs.edit().putString("last", value?.name).putLong("opened:${value?.name}", System.currentTimeMillis()).apply()
        }

    fun position(book: File): Long = prefs.getLong("pos:${book.name}", 0)

    /** Сколько прочитано, 0..100; −1 — книгу ещё не открывали. */
    fun progress(book: File): Int = prefs.getInt("pct:${book.name}", -1)

    fun saveProgress(book: File, percent: Int) = prefs.edit().putInt("pct:${book.name}", percent.coerceIn(0, 100)).apply()

    /** Удаляет книгу вместе с местом чтения. */
    fun delete(book: File) {
        book.delete()
        coverFile(book).delete()
        val e = prefs.edit().remove("pos:${book.name}").remove("pct:${book.name}").remove("opened:${book.name}")
            .remove("info:${book.name}").remove("filemeta:${book.name}")
        if (prefs.getString("last", null) == book.name) e.remove("last")
        e.apply()
    }

    // ---- Сведения о книге ----

    /** Название и автор для показа. Пустой автор — не известен. */
    class Info(val title: String, val author: String)

    private val covers = File(ctx.filesDir, "covers")
    fun coverFile(book: File) = File(covers, book.name + ".jpg")

    /** Правки пользователя: «название\nавтор»; null — не правили. */
    private fun edited(book: File): Info? =
        prefs.getString("info:${book.name}", null)?.split('\n', limit = 2)?.let { Info(it[0], it.getOrElse(1) { "" }) }

    /** Данные из самого файла: берутся один раз (нужен разбор книги, поэтому не в главном потоке) и запоминаются. */
    fun fileInfo(book: File): Info {
        val cached = prefs.getString("filemeta:${book.name}", null)
        val raw = cached ?: (Core.meta(book.path) ?: "\n").also { prefs.edit().putString("filemeta:${book.name}", it).apply() }
        val p = raw.split('\n', limit = 2)
        return Info(p[0].ifBlank { cleanTitle(book) }, p.getOrElse(1) { "" })
    }

    /** Без разбора книги: правки, иначе уже известное из файла, иначе имя файла. */
    fun info(book: File): Info = edited(book) ?: prefs.getString("filemeta:${book.name}", null)?.let {
        val p = it.split('\n', limit = 2)
        Info(p[0].ifBlank { cleanTitle(book) }, p.getOrElse(1) { "" })
    } ?: Info(cleanTitle(book), "")

    fun needsFileInfo(book: File) = edited(book) == null && prefs.getString("filemeta:${book.name}", null) == null

    fun setInfo(book: File, title: String, author: String) =
        prefs.edit().putString("info:${book.name}", "${title.trim().replace('\n', ' ')}\n${author.trim().replace('\n', ' ')}").apply()

    /** Вернуть то, что записано в файле книги, и убрать скачанную обложку. */
    fun resetInfo(book: File) {
        prefs.edit().remove("info:${book.name}").apply()
        coverFile(book).delete()
    }

    fun saveCover(book: File, bytes: ByteArray) {
        covers.mkdirs()
        writeSafely(coverFile(book)) { it.write(bytes) }
    }

    /** Можно звать из любого потока. */
    fun savePosition(book: File, pos: Long) = prefs.edit().putLong("pos:${book.name}", pos).apply()

    companion object {
        /**
         * Запись «всё или ничего»: во временный файл, сброс на диск, переименование.
         * Если телефон выключится посреди записи, останется прежний файл, а не пустой.
         */
        fun writeSafely(to: File, write: (java.io.OutputStream) -> Unit) {
            val tmp = File(to.path + ".tmp")
            java.io.FileOutputStream(tmp).use { out ->
                write(out)
                out.flush()
                out.fd.sync()
            }
            if (!tmp.renameTo(to)) {
                tmp.delete()
                error("не удалось записать ${to.name}")
            }
        }

        fun isBook(name: String): Boolean {
            val n = name.lowercase()
            return n.endsWith(".fb2") || n.endsWith(".fb2.zip") || n.endsWith(".zip") || n.endsWith(".epub")
        }

        /** Имя без расширения — для заголовка и обложки. */
        fun cleanTitle(f: File): String =
            f.name.replace(Regex("(?i)\\.fb2(\\.zip)?$|\\.zip$|\\.epub$"), "").replace('_', ' ')

        /** Имя для заголовков окон: у EPUB пометка. */
        fun title(f: File): String {
            val name = f.name.replace(Regex("(?i)\\.fb2(\\.zip)?$|\\.zip$|\\.epub$"), "")
            return if (f.name.lowercase().endsWith(".epub")) "$name · EPUB" else name
        }
    }
}
