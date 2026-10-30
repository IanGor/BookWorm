package app.bookworm

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File

/**
 * Настройки чтения (SPEC §4): гарнитура, ступень размера, тема, способ листания —
 * и шрифты: встроенные из APK и свои из папки, выбранной через «Обзор».
 */
class Settings(private val ctx: Context) {
    private val prefs = ctx.getSharedPreferences("reader", Context.MODE_PRIVATE)

    /** Встроенные шрифты (из APK) и копии своих шрифтов из выбранной папки. */
    val builtinDir = File(ctx.filesDir, "fonts")
    val userDir = File(ctx.filesDir, "userfonts")
    val mono: String get() = File(builtinDir, MONO).path

    /** Ступени размера и ступень по умолчанию — из ядра. */
    val steps: List<Float>
    private val defaultStep: Int

    init {
        val v = Core.sizeSteps().split(' ').mapNotNull { it.toFloatOrNull() }
        steps = v.dropLast(1)
        defaultStep = v.last().toInt()
    }

    var family: String
        get() = prefs.getString("family", DEFAULT_FAMILY) ?: DEFAULT_FAMILY
        set(v) = prefs.edit().putString("family", v).apply()

    var familyIsUser: Boolean
        get() = prefs.getBoolean("familyUser", false)
        set(v) = prefs.edit().putBoolean("familyUser", v).apply()

    var step: Int
        get() = prefs.getInt("step", defaultStep).coerceIn(0, steps.size - 1)
        set(v) = prefs.edit().putInt("step", v.coerceIn(0, steps.size - 1)).apply()

    val stepPx: Float get() = steps[step]

    /** «auto», «day» или «night». */
    var theme: String
        get() = prefs.getString("theme", "auto") ?: "auto"
        set(v) = prefs.edit().putString("theme", v).apply()

    /** Яркость экрана в приложении 0..1, независимо от системной; −1 — ещё не задавали. */
    var brightness: Float
        get() = prefs.getFloat("brightness", -1f)
        set(v) = prefs.edit().putFloat("brightness", v).apply()

    var scroll: Boolean
        get() = prefs.getBoolean("scroll", false)
        set(v) = prefs.edit().putBoolean("scroll", v).apply()

    /** Яркость, с которой начинать: своя, а если её не задавали — системная. */
    fun brightnessOrSystem(): Float {
        val b = brightness
        if (b >= 0f) return b
        val sys = runCatching {
            android.provider.Settings.System.getInt(ctx.contentResolver, android.provider.Settings.System.SCREEN_BRIGHTNESS)
        }.getOrDefault(128)
        return (sys / 255f).coerceIn(0.03f, 1f)
    }

    /** Конфигурация с принудительной темой: «День» и «Ночь» не зависят от системной, «Авто» — null. */
    fun themeConfig(base: Context): android.content.res.Configuration? {
        val night = when (theme) {
            "day" -> android.content.res.Configuration.UI_MODE_NIGHT_NO
            "night" -> android.content.res.Configuration.UI_MODE_NIGHT_YES
            else -> return null
        }
        val c = android.content.res.Configuration(base.resources.configuration)
        c.uiMode = (c.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK.inv()) or night
        return c
    }

    /** Всё, что требует переверстать книгу: если оно изменилось, читалка откроет книгу заново. */
    val layoutKey: String get() = "$family|$familyIsUser|$step|$scroll"

    class Family(val name: String, val cyrillic: Boolean, val faces: String, val user: Boolean) {
        val label: String
            get() = name + (if (user) " · свой" else "") + (if (cyrillic) "" else " · без кириллицы")
    }

    /** Гарнитуры: встроенные (Literata первой), затем свои. Фоновый поток: ядро читает файлы. */
    fun families(): List<Family> {
        fun scan(dir: File, user: Boolean): List<Family> {
            val files = dir.listFiles { f -> isFont(f.name) && f.name != MONO && f.length() > 0 }.orEmpty().sortedBy { it.name }
            if (files.isEmpty()) return emptyList()
            return Core.fontFamilies(files.joinToString("\n") { it.path }).orEmpty().lines()
                .filter { it.isNotBlank() }
                .map { it.split('\t') }
                .filter { it.size >= 6 }
                .map { Family(it[0], it[1] == "1", it.subList(2, 6).joinToString("\n"), user) }
        }
        return scan(builtinDir, false).sortedBy { it.name != DEFAULT_FAMILY } + scan(userDir, true)
    }

    /** Выбранная гарнитура; если её больше нет (сменили папку) — Literata. */
    fun current(all: List<Family>): Family? =
        all.firstOrNull { it.name == family && it.user == familyIsUser }
            ?: all.firstOrNull { it.name == family }
            ?: all.firstOrNull { it.name == DEFAULT_FAMILY }
            ?: all.firstOrNull()

    /**
     * Копирует TTF/OTF из выбранной папки (и вложенных) к себе: ядру нужны пути к файлам.
     * Прежние свои шрифты заменяются. Возвращает число файлов. Фоновый поток.
     */
    fun importFolder(tree: Uri): Int {
        userDir.mkdirs()
        userDir.listFiles()?.forEach { it.delete() }
        var n = 0
        val cols = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )
        fun walk(docId: String, depth: Int) {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, docId)
            ctx.contentResolver.query(children, cols, null, null, null)?.use { c ->
                while (c.moveToNext()) {
                    val id = c.getString(0)
                    val name = c.getString(1) ?: continue
                    if (c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (depth < 3) walk(id, depth + 1)
                        continue
                    }
                    if (!isFont(name)) continue
                    val uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)
                    val to = File(userDir, name.replace(Regex("[/\\\\:]"), "_"))
                    ctx.contentResolver.openInputStream(uri)?.use { i -> Books.writeSafely(to) { i.copyTo(it) } }
                    n++
                }
            }
        }
        walk(DocumentsContract.getTreeDocumentId(tree), 0)
        return n
    }

    companion object {
        const val DEFAULT_FAMILY = "Literata"
        const val MONO = "LiberationMono-Regular.ttf"

        fun isFont(name: String): Boolean {
            val n = name.lowercase()
            return n.endsWith(".ttf") || n.endsWith(".otf")
        }
    }
}
