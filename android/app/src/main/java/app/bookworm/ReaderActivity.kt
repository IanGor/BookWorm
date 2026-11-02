package app.bookworm

import android.app.Activity
import android.app.Dialog
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.TextUtils
import android.util.Log
import android.view.Gravity
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.view.animation.DecelerateInterpolator
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import android.widget.Toast
import android.widget.EditText
import java.io.File
import java.nio.ByteBuffer
import java.util.concurrent.Executors

/**
 * Экран чтения. Всё, что касается книги, делает ядро в одном фоновом потоке
 * (`worker`); здесь — жесты, кэш готовых страниц и панель управления.
 *
 * Два способа листания (Настройки → Листание):
 * - страницы: горизонтальные свайпы и кнопки громкости;
 * - непрерывная прокрутка пальцем с инерцией, кнопки громкости сдвигают текст на экран.
 * В обоих режимах касание в верхней трети — назад, в нижней — вперёд.
 * Касание в центральной трети по обеим осям показывает панель: «Библиотека», название,
 * оглавление, «Аа» (настройки), внизу ползунок по книге, сверху системная строка состояния.
 * Вертикальный свайп по левому краю меняет
 * размер текста, по правому — яркость.
 */
class ReaderActivity : ThemedActivity(), PageView.Listener, StripView.Host {
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var books: Books
    private lateinit var settings: Settings
    private lateinit var root: FrameLayout
    private lateinit var pageView: PageView
    private lateinit var stripView: StripView
    private lateinit var hud: TextView
    private var topBar: View? = null
    private var bottomBar: View? = null
    private var scrubber: SeekBar? = null
    private var scrubLabel: TextView? = null
    private var chromeShown = false
    private var topInset = 0
    private var bottomInset = 0

    // Поля ниже меняются только в главном потоке.
    private var book: File? = null
    private var pendingReport: String? = null
    /** Настройки, с которыми книга свёрстана: изменились — переверстываем при возврате из настроек. */
    private var openedKey = ""
    private var pendingBook: File? = null
    private var scrollMode = false
    private var page = 0
    private var count = 0
    private var night = false
    /** Растёт при каждой переверстке или смене темы: устаревшие результаты фона отбрасываются. */
    private var gen = 0
    private val cache = HashMap<Int, PageImage>()
    private val inFlight = HashSet<Int>()
    private val tiles = object : LinkedHashMap<Long, Bitmap>(16, 0.75f, true) {
        override fun removeEldestEntry(e: MutableMap.MutableEntry<Long, Bitmap>) = size > MAX_TILES
    }
    private val tilesInFlight = HashSet<Long>()
    private val chapterNotes = HashMap<Int, List<NoteArea>>()
    private var footerBitmap: Bitmap? = null
    private var pendingUri: Uri? = null
    private var prepared = false
    private var started = false
    private var openedSize = 0 to 0

    // Поля ниже — только в фоновом потоке.
    private var handle = 0L
    private var width = 0
    private var height = 0
    private var buffer: ByteBuffer? = null

    private class PageImage(val bitmap: Bitmap, val notes: List<NoteArea>)

    private class NoteArea(val x0: Float, val y0: Float, val x1: Float, val y1: Float, val id: String) {
        fun contains(x: Float, y: Float) = x in x0..x1 && y in y0..y1
    }

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        books = Books(this)
        settings = Settings(this)
        pendingReport = state?.getString("pendingReport")
        night = isNight()
        pageView = PageView(this, this)
        stripView = StripView(this, this).apply {
            background = getColor(R.color.page)
            visibility = View.GONE
        }
        hud = TextView(this).apply {
            textSize = 22f
            setTextColor(getColor(R.color.ink))
            val pad = (18 * resources.displayMetrics.density).toInt()
            setPadding(pad * 2, pad, pad * 2, pad)
            background = android.graphics.drawable.GradientDrawable().apply {
                cornerRadius = pad.toFloat()
                setColor(getColor(R.color.panel))
            }
            visibility = View.GONE
        }
        root = FrameLayout(this).apply {
            addView(pageView)
            addView(stripView)
            addView(hud, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
        }
        setContentView(root)
        root.onInsets { top, bottom ->
            if (topInset != top || bottomInset != bottom || topBar == null) {
                topInset = top
                bottomInset = bottom
                buildChrome()
            }
        }
        applyBrightness()
        updateSystemBars()
        pendingUri = incomingUri(intent)
        pendingBook = incomingBook(intent)
        pageView.message("Подготовка…", fg())
        worker.execute {
            books.prepare()
            main.post {
                prepared = true
                start()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        pendingUri = incomingUri(intent)
        pendingBook = incomingBook(intent)
        if (pendingUri == null && pendingBook == null) return
        if (prepared) start()
    }

    private fun incomingUri(i: Intent?): Uri? = when (i?.action) {
        Intent.ACTION_VIEW -> i.data
        Intent.ACTION_SEND -> i.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else -> null
    }

    /** Книга из библиотеки: файл уже у нас. */
    private fun incomingBook(i: Intent?): File? =
        i?.getStringExtra(EXTRA_BOOK)?.let { File(books.dir, it) }?.takeIf { it.exists() }

    /** Что открыть: присланный файл, книгу из библиотеки, иначе текущую или последнюю; нет книги — в библиотеку. */
    private fun start() {
        if (!prepared || root.width == 0) return
        started = true
        val uri = pendingUri
        pendingUri = null
        val chosen = pendingBook
        pendingBook = null
        if (uri != null) {
            message("Копирую книгу…")
            worker.execute {
                val f = runCatching { books.import(uri) }
                main.post {
                    f.onSuccess { open(it) }.onFailure {
                        toast("Не удалось скопировать книгу: ${it.message}")
                        toLibrary()
                    }
                }
            }
            return
        }
        val target = chosen ?: book ?: books.last
        if (target != null) open(target) else toLibrary()
    }

    /** Назад к книгам. Если читалку открыли из другого приложения, библиотеки под ней нет — открываем. */
    private fun toLibrary() {
        if (isTaskRoot) startActivity(Intent(this, LibraryActivity::class.java))
        finish()
    }

    override fun onResize(width: Int, height: Int) {
        // Окно меняет размер, пока скрываются системные панели: верстаем под окончательный.
        main.removeCallbacks(reopenOnResize)
        main.postDelayed(reopenOnResize, 150)
    }

    private val reopenOnResize = Runnable {
        val b = book
        when {
            !started -> start()
            b != null && openedSize != root.width to root.height -> open(b)
        }
    }

    private fun message(text: String) {
        stripView.visibility = View.GONE
        pageView.visibility = View.VISIBLE
        pageView.message(text, fg())
    }

    /** Открывает (или переверстывает после смены настроек) книгу на сохранённом месте. */
    private fun open(file: File) {
        val w = root.width
        val h = root.height
        if (w == 0) return
        openedSize = w to h
        openedKey = settings.layoutKey
        val g = ++gen
        val dark = night
        val scroll = settings.scroll
        val step = settings.stepPx
        val bionic = settings.bionic
        clearImages()
        message("Вёрстка…")
        worker.execute {
            if (handle != 0L) Core.close(handle)
            handle = 0
            width = w
            height = h
            val family = settings.current(settings.families())
            if (family == null) {
                main.post { toast("Нет ни одного шрифта") }
                return@execute
            }
            val t0 = SystemClock.elapsedRealtime()
            val hd = Core.open(file.path, family.faces, settings.mono, w, h, step, scroll, bionic)
            if (hd == 0L) {
                val err = Core.lastError()
                main.post {
                    toast("Не открывается «${Books.title(file)}»: $err")
                    if (g == gen) {
                        book = null
                        count = 0
                        toLibrary()
                    }
                }
                return@execute
            }
            handle = hd
            if (books.needsFileInfo(file)) books.fileInfo(file)
            val n = Core.pageCount(hd)
            val pos = books.position(file)
            val p = Core.pageOf(hd, pos).coerceIn(0, maxOf(n - 1, 0))
            val t1 = SystemClock.elapsedRealtime()
            Log.i(TAG, "«${file.name}»: ${family.name}, ступень $step, страниц $n, вёрстка ${t1 - t0} мс")
            if (scroll) {
                val heights = FloatArray(n) { Core.pageHeight(hd, it) }
                val top = Core.topOf(hd, p, pos)
                main.post {
                    if (g != gen) return@post
                    opened(file, n, true)
                    stripView.heights = heights
                    stripView.jumpTo(p, top)
                }
            } else {
                val img = renderPage(p, dark)
                main.post {
                    if (g != gen) return@post
                    opened(file, n, false)
                    page = p
                    if (p > 0) books.saveProgress(file, percent(p))
                    if (img != null) {
                        cache[p] = img
                        pageView.show(img.bitmap)
                    } else {
                        pageView.message("Пустая книга", fg())
                    }
                    prefetch()
                }
            }
        }
    }

    private fun opened(file: File, n: Int, scroll: Boolean) {
        book = file
        books.last = file
        count = n
        scrollMode = scroll
        pageView.visibility = if (scroll) View.GONE else View.VISIBLE
        stripView.visibility = if (scroll) View.VISIBLE else View.GONE
        buildChrome()
    }

    /** Сколько прочитано, по номеру страницы (в прокрутке — по высоте глав), 0..100. */
    private fun percent(p: Int) = if (count > 1) p * 100 / (count - 1) else 0

    private fun clearImages() {
        cache.clear()
        inFlight.clear()
        tiles.clear()
        tilesInFlight.clear()
        chapterNotes.clear()
        footerBitmap = null
    }

    /** Фоновый поток: общий прямой буфер для пикселей из ядра. */
    private fun buffer(size: Int): ByteBuffer {
        val buf = buffer?.takeIf { it.capacity() >= size } ?: ByteBuffer.allocateDirect(size).also { buffer = it }
        buf.rewind()
        return buf
    }

    private fun bitmap(w: Int, h: Int, buf: ByteBuffer): Bitmap {
        buf.rewind()
        return Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).apply { copyPixelsFromBuffer(buf) }
    }

    private fun noteAreas(p: Int): List<NoteArea> =
        Core.noteAreas(handle, p).orEmpty().lines().mapNotNull { l ->
            val f = l.split(' ', limit = 5)
            if (f.size < 5) null else NoteArea(f[0].toFloat(), f[1].toFloat(), f[2].toFloat(), f[3].toFloat(), f[4])
        }

    /** Фоновый поток: страница из ядра в Bitmap вместе с областями знаков сносок. */
    private fun renderPage(p: Int, dark: Boolean): PageImage? {
        if (handle == 0L) return null
        val buf = buffer(width * height * 4)
        if (!Core.render(handle, p, dark, buf)) return null
        return PageImage(bitmap(width, height, buf), noteAreas(p))
    }

    // ---- Страницы ----

    private fun go(target: Int) {
        if (count == 0 || target < 0 || target >= count || target == page) return
        if (chromeShown) hideChrome()
        page = target
        val img = cache[target]
        if (img != null) pageView.show(img.bitmap) else request(target)
        val file = book
        if (file != null) books.saveProgress(file, percent(target))
        worker.execute {
            if (file != null && handle != 0L) books.savePosition(file, Core.pageStart(handle, target))
        }
        prefetch()
    }

    /** Соседние страницы рисуются заранее, чтобы листание было мгновенным. */
    private fun prefetch() {
        cache.keys.retainAll(setOf(page - 1, page, page + 1))
        for (p in listOf(page + 1, page - 1)) {
            if (p in 0 until count && p !in cache) request(p)
        }
    }

    private fun request(p: Int) {
        if (!inFlight.add(p)) return
        val g = gen
        val dark = night
        worker.execute {
            val img = renderPage(p, dark)
            main.post {
                if (g != gen) return@post
                inFlight.remove(p)
                if (img == null) return@post
                if (p in page - 1..page + 1) cache[p] = img
                if (p == page) pageView.show(img.bitmap)
            }
        }
    }

    // ---- Прокрутка ----

    override fun tile(chapter: Int, index: Int): Bitmap? {
        val key = (chapter.toLong() shl 32) or index.toLong()
        val bmp = tiles[key]
        if (bmp == null) requestTile(chapter, index)
        // Следующая плитка — заранее: прокрутка вниз не должна натыкаться на пустоту.
        val th = stripView.tileHeight
        val next = (chapter.toLong() shl 32) or (index + 1).toLong()
        if ((index + 1) * th < stripView.heights.getOrElse(chapter) { 0f } && next !in tiles) {
            requestTile(chapter, index + 1)
        }
        return bmp
    }

    private fun requestTile(chapter: Int, index: Int) {
        val key = (chapter.toLong() shl 32) or index.toLong()
        if (!tilesInFlight.add(key)) return
        val g = gen
        val dark = night
        val th = stripView.tileHeight
        val needNotes = chapter !in chapterNotes
        worker.execute {
            if (handle == 0L) return@execute
            val buf = buffer(width * th * 4)
            val ok = Core.renderStrip(handle, chapter, index * th.toFloat(), th, dark, buf)
            val bmp = if (ok) bitmap(width, th, buf) else null
            val notes = if (needNotes) noteAreas(chapter) else null
            main.post {
                if (g != gen) return@post
                tilesInFlight.remove(key)
                if (notes != null) chapterNotes[chapter] = notes
                if (bmp != null) {
                    tiles[key] = bmp
                    stripView.invalidate()
                }
            }
        }
    }

    override fun footer(): Bitmap? = footerBitmap

    override fun onSettled(chapter: Int, top: Float) {
        val file = book ?: return
        val h = stripView.heights
        val total = h.sum()
        if (total > 0f) books.saveProgress(file, ((h.take(chapter).sum() + top) * 100f / total).toInt())
        val g = gen
        val dark = night
        worker.execute {
            if (handle == 0L) return@execute
            books.savePosition(file, Core.posAt(handle, chapter, top))
            val fh = Core.footerHeight(handle)
            val buf = buffer(width * fh * 4)
            if (!Core.renderFooter(handle, chapter, top, dark, buf)) return@execute
            val bmp = bitmap(width, fh, buf)
            main.post {
                if (g != gen) return@post
                footerBitmap = bmp
                stripView.invalidate()
            }
        }
    }

    /** Экранная точка в режиме прокрутки → (глава, высота в главе). */
    private fun stripPoint(y: Float): Pair<Int, Float> {
        val h = stripView.heights
        var ch = stripView.chapter
        var cy = stripView.top + y
        while (ch < h.size - 1 && cy >= h[ch]) {
            cy -= h[ch]
            ch++
        }
        return ch to cy
    }

    /** На экран вниз (или вверх) с запасом в пару строк, чтобы не терять нить. */
    private fun scrollScreen(forward: Boolean) {
        val step = root.height * 0.85f
        stripView.scrollBy(if (forward) step else -step)
    }

    /** Переход к странице (или к позиции в главе при прокрутке). */
    private fun jumpTo(page: Int, pos: Long) {
        if (!scrollMode) {
            go(page)
            return
        }
        val g = gen
        worker.execute {
            val top = Core.topOf(handle, page, pos)
            main.post { if (g == gen) stripView.jumpTo(page, top) }
        }
    }

    // ---- Жесты ----

    override fun onLongPress(x: Float, y: Float) {
        val file = book ?: return
        if (count == 0 || chromeShown) return
        val g = gen
        val (part, cy) = if (scrollMode) stripPoint(y) else page to y
        val configuration = "Шрифт: ${settings.family}; ступень: ${settings.stepPx}; тема: ${settings.theme}; " +
            "ночной режим: $night; бионическое чтение: ${settings.bionic}; прокрутка: $scrollMode"
        worker.execute {
            if (g != gen || handle == 0L) return@execute
            val block = Core.blockAt(handle, part, x, cy)
            val text = Core.blockText(handle, block) ?: return@execute
            if (text.isBlank()) return@execute
            val diagnostics = Core.typographyReport(handle, block) ?: return@execute
            main.post {
                if (g != gen || isDestroyed) return@post
                val selectable = label(this, text, 18f, fg()).apply {
                    setPadding(dp(20), dp(12), dp(20), dp(12))
                    setTextIsSelectable(true)
                }
                AlertDialog.Builder(this)
                    .setTitle("Ошибка оформления")
                    .setMessage("Выделите фрагмент долгим нажатием. Без выделения в отчёт попадёт весь абзац.")
                    .setView(ScrollView(this).apply { addView(selectable) })
                    .setNegativeButton("Отмена", null)
                    .setPositiveButton("Описать ошибку") { _, _ ->
                        val a = minOf(selectable.selectionStart, selectable.selectionEnd)
                        val b = maxOf(selectable.selectionStart, selectable.selectionEnd)
                        val selected = if (a >= 0 && b > a) text.substring(a, b) else text
                        describeTypography(file, selected, diagnostics, configuration)
                    }.show()
            }
        }
    }

    private fun describeTypography(file: File, selected: String, diagnostics: String, configuration: String) {
        val input = EditText(this).apply {
            hint = "Что выглядит неправильно и как должно быть?"
            inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE or
                android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            minLines = 3
            setTextColor(fg())
        }
        val dialog = AlertDialog.Builder(this)
            .setTitle("Комментарий к фрагменту")
            .setView(input)
            .setNegativeButton("Отмена", null)
            .setPositiveButton("Сохранить промпт", null).create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                val comment = input.text.toString().trim()
                if (comment.isEmpty()) { input.error = "Опишите ошибку оформления"; return@setOnClickListener }
                val prompt = """
                    Исправь ошибку оформления в Android-читалке BookWorm (репозиторий IanGor/BookWorm).
                    Найди причину в разборе книги, типографе, подборе строк или отрисовке. Сохрани содержание книги.
                    Воспроизведи проблему на приведённых данных и добавь проверку исправления.

                    Комментарий пользователя:
                    $comment

                    Выделенный фрагмент:
                    $selected

                    Файл: ${file.name}
                    BookWorm ${packageManager.getPackageInfo(packageName, 0).versionName}; Android ${android.os.Build.VERSION.RELEASE} (API ${android.os.Build.VERSION.SDK_INT})
                    Экран: ${root.width} × ${root.height} px; плотность: ${resources.displayMetrics.density}
                    $configuration

                    Данные движка (текст после типографа, исходные стили и CSS, соседние блоки, строки):
                    $diagnostics
                """.trimIndent()
                dialog.dismiss()
                worker.execute {
                    val result = runCatching {
                        val dir = File(filesDir, "typography-reports").apply { mkdirs() }
                        File.createTempFile("bookworm-report-", ".txt", dir).also { f ->
                            Books.writeSafely(f) { it.write(prompt.toByteArray(Charsets.UTF_8)) }
                        }
                    }
                    main.post {
                        if (isDestroyed) return@post
                        result.onSuccess { f -> pendingReport = f.name; exportReport() }
                            .onFailure { toast("Не удалось сохранить промпт: ${it.message}") }
                    }
                }
            }
        }
        dialog.show()
    }

    private fun exportReport() {
        val name = pendingReport ?: return
        startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "text/plain"
            putExtra(Intent.EXTRA_TITLE, name)
        }, REPORT_DOCUMENT)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REPORT_DOCUMENT) return
        val name = pendingReport ?: return
        val file = File(File(filesDir, "typography-reports"), File(name).name)
        val uri = data?.data
        if (resultCode != Activity.RESULT_OK || uri == null) {
            Sheet(this, "Промпт сохранён в приложении")
                .action("Сохранить в файлы") { exportReport() }
                .action("Копировать промпт") {
                    getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Ошибка оформления", file.readText()))
                    toast("Промпт скопирован")
                }.show()
            return
        }
        worker.execute {
            val result = runCatching {
                val stream = contentResolver.openOutputStream(uri, "wt") ?: error("файл не открывается")
                stream.use { out -> file.inputStream().use { it.copyTo(out) } }
            }
            main.post {
                result.onSuccess { toast("Промпт сохранён в выбранный файл") }
                    .onFailure { toast("Не удалось экспортировать: ${it.message}"); exportReport() }
            }
        }
    }

    override fun onSaveInstanceState(out: Bundle) {
        out.putString("pendingReport", pendingReport)
        super.onSaveInstanceState(out)
    }

    override fun onTap(x: Float, y: Float) {
        // Пока панель открыта, любое касание страницы её закрывает и больше ничего не делает.
        if (chromeShown) {
            hideChrome()
            return
        }
        val note = if (scrollMode) {
            val (ch, cy) = stripPoint(y)
            chapterNotes[ch]?.firstOrNull { it.contains(x, cy) }
        } else {
            cache[page]?.notes?.firstOrNull { it.contains(x, y) }
        }
        if (note != null) {
            showNote(note.id)
            return
        }
        val upper = root.height / 3f
        val lower = root.height * 2f / 3f
        if (y >= upper && y < lower) {
            if (x >= root.width / 3f && x < root.width * 2f / 3f) showChrome()
            return
        }
        val forward = y >= lower
        if (scrollMode) scrollScreen(forward) else go(if (forward) page + 1 else page - 1)
    }

    override fun onSwipe(forward: Boolean) = go(if (forward) page + 1 else page - 1)

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val forward = when (keyCode) {
            KeyEvent.KEYCODE_VOLUME_DOWN -> true
            KeyEvent.KEYCODE_VOLUME_UP -> false
            else -> return super.onKeyDown(keyCode, event)
        }
        if (chromeShown) hideChrome()
        if (scrollMode) scrollScreen(forward) else go(if (forward) page + 1 else page - 1)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean =
        keyCode == KeyEvent.KEYCODE_VOLUME_DOWN || keyCode == KeyEvent.KEYCODE_VOLUME_UP || super.onKeyUp(keyCode, event)

    // ---- Панель управления: библиотека, оглавление, настройки, ползунок ----

    /**
     * Полупрозрачная панель сверху и снизу поверх страницы. Появляется по касанию центра,
     * уходит по касанию страницы. Строится заново при смене темы и размеров окна.
     */
    private fun buildChrome() {
        topBar?.let { root.removeView(it) }
        bottomBar?.let { root.removeView(it) }
        val shown = chromeShown
        val panel = (getColor(R.color.panel) and 0x00FFFFFF) or (0xFA shl 24)
        val accent = getColor(R.color.accent)
        val ink = getColor(R.color.ink)
        val muted = getColor(R.color.muted)
        val line = getColor(R.color.separator)

        val titleText = TextView(this).apply {
            text = book?.let { books.info(it).title } ?: ""
            textSize = 15f
            typeface = weight(600)
            setTextColor(ink)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            gravity = Gravity.CENTER
            setPadding(dp(8), 0, dp(8), 0)
        }
        val back = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            isClickable = true
            contentDescription = "Библиотека"
            setPadding(dp(8), 0, dp(12), 0)
            addView(ImageView(this@ReaderActivity).apply { setImageDrawable(Icon(Icon.Kind.BACK, accent)) }, LinearLayout.LayoutParams(dp(22), dp(22)))
            addView(label(this@ReaderActivity, "Библиотека", 17f, accent), LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(2) })
            setOnClickListener { toLibrary() }
            pressDim()
        }
        val toc = iconButton(this, Icon.Kind.LIST, "Оглавление", accent) { showToc() }
        val aa = label(this, "Aa", 19f, accent, 600).apply {
            gravity = Gravity.CENTER
            contentDescription = "Настройки"
            isClickable = true
            setOnClickListener { openSettings() }
            pressDim()
        }
        val top = FrameLayout(this).apply {
            setBackgroundColor(panel)
            isClickable = true
            setPadding(0, topInset, 0, 0)
            addView(LinearLayout(this@ReaderActivity).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                addView(back, LinearLayout.LayoutParams(-2, -1))
                addView(titleText, LinearLayout.LayoutParams(0, -1, 1f))
                addView(toc)
                addView(aa, LinearLayout.LayoutParams(dp(48), -1))
            }, FrameLayout.LayoutParams(-1, dp(48), Gravity.BOTTOM).apply { rightMargin = dp(4) })
            addView(View(this@ReaderActivity).apply { setBackgroundColor(line) }, FrameLayout.LayoutParams(-1, 1, Gravity.BOTTOM))
        }

        val seek = SeekBar(this).apply {
            val tint = ColorStateList.valueOf(accent)
            progressTintList = tint
            thumbTintList = tint
            progressBackgroundTintList = ColorStateList.valueOf((muted and 0x00FFFFFF) or (0x55 shl 24))
            max = maxOf(count - 1, 0)
            progress = currentPart()
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar, p: Int, fromUser: Boolean) {
                    if (fromUser) scrubLabel?.text = partLabel(p)
                }

                override fun onStartTrackingTouch(s: SeekBar) = Unit

                override fun onStopTrackingTouch(s: SeekBar) {
                    s.tick()
                    scrubTo(s.progress)
                }
            })
        }
        val lab = label(this, partLabel(currentPart()), 13f, muted).apply { gravity = Gravity.CENTER }
        val bottom = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(panel)
            isClickable = true
            setPadding(dp(16), dp(10), dp(16), bottomInset + dp(6))
            addView(lab, LinearLayout.LayoutParams(-1, -2))
            addView(seek, LinearLayout.LayoutParams(-1, dp(40)))
        }
        val bottomWrap = FrameLayout(this).apply {
            addView(bottom, FrameLayout.LayoutParams(-1, -2))
            addView(View(this@ReaderActivity).apply { setBackgroundColor(line) }, FrameLayout.LayoutParams(-1, 1, Gravity.TOP))
        }
        scrubber = seek
        scrubLabel = lab
        root.addView(top, FrameLayout.LayoutParams(-1, topInset + dp(52), Gravity.TOP))
        root.addView(bottomWrap, FrameLayout.LayoutParams(-1, -2, Gravity.BOTTOM))
        topBar = top
        bottomBar = bottomWrap
        // Если панель была открыта (смена темы), остаётся открытой; иначе её нет на экране.
        top.visibility = if (shown) View.VISIBLE else View.GONE
        bottomWrap.visibility = top.visibility
    }

    private fun currentPart() = if (scrollMode) stripView.chapter else page

    private fun partLabel(p: Int): String {
        if (count == 0) return ""
        val n = p.coerceIn(0, count - 1) + 1
        return if (scrollMode) "Часть $n из $count" else "Страница $n из $count · ${percent(n - 1)} %"
    }

    private fun scrubTo(p: Int) {
        if (count == 0) return
        if (scrollMode) stripView.jumpTo(p, 0f) else go(p)
    }

    private fun showChrome() {
        val t = topBar ?: return
        val b = bottomBar ?: return
        if (book == null || chromeShown) return
        chromeShown = true
        scrubber?.max = maxOf(count - 1, 0)
        scrubber?.progress = currentPart()
        scrubLabel?.text = partLabel(currentPart())
        for (v in listOf(t, b)) {
            v.visibility = View.VISIBLE
            v.alpha = 0f
        }
        updateSystemBars()
        t.post {
            t.translationY = -t.height / 3f
            b.translationY = b.height / 3f
            for (v in listOf(t, b)) {
                v.animate().alpha(1f).translationY(0f).setDuration(240).setInterpolator(DecelerateInterpolator(2f)).start()
            }
        }
    }

    private fun hideChrome() {
        val t = topBar ?: return
        val b = bottomBar ?: return
        if (!chromeShown) return
        chromeShown = false
        updateSystemBars()
        t.animate().alpha(0f).translationY(-t.height / 3f).setDuration(180).withEndAction { if (!chromeShown) t.visibility = View.GONE }.start()
        b.animate().alpha(0f).translationY(b.height / 3f).setDuration(180).withEndAction { if (!chromeShown) b.visibility = View.GONE }.start()
    }

    private fun openSettings() {
        hideChrome()
        startActivity(Intent(this, SettingsActivity::class.java))
    }

    /** Оглавление нижним листом: текущий раздел отмечен галочкой и показан сразу. */
    private fun showToc() {
        if (book == null) return
        val scroll = scrollMode
        val (ch, top, p) = Triple(stripView.chapter, stripView.top, page)
        worker.execute {
            // Текущий раздел — по месту чтения: позиции сравниваются как числа (блок, символ).
            val here = if (scroll) Core.posAt(handle, ch, top) else Core.pageStart(handle, p)
            val toc = Core.toc(handle).orEmpty().lines().filter { it.isNotBlank() }.map { it.split('\t', limit = 4) }
            main.post {
                if (toc.isEmpty()) {
                    toast("В книге нет оглавления")
                    return@post
                }
                hideChrome()
                val current = toc.indexOfLast { it[2].toLong() <= here }
                val sheet = Sheet(this, "Содержание")
                toc.forEachIndexed { i, e ->
                    sheet.choice(e[3], i == current, indent = maxOf(e[0].toInt() - 1, 0)) { jumpTo(e[1].toInt(), e[2].toLong()) }
                }
                sheet.show()
            }
        }
    }

    // ---- Сноска ----

    private fun showNote(id: String) {
        val dark = night
        worker.execute {
            val h = Core.noteHeight(handle, id)
            if (h <= 0) return@execute
            val buf = ByteBuffer.allocateDirect(width * h * 4)
            if (!Core.renderNote(handle, id, dark, buf)) return@execute
            val bmp = bitmap(width, h, buf)
            main.post { showNotePopup(bmp) }
        }
    }

    /** Попап снизу, как у Kindle: текст сноски свёрстан ядром; касание где угодно закрывает. */
    private fun showNotePopup(bmp: Bitmap) {
        val d = Dialog(this, android.R.style.Theme_Translucent_NoTitleBar)
        val image = ImageView(this).apply {
            setImageBitmap(bmp)
            adjustViewBounds = true
            scaleType = ImageView.ScaleType.FIT_START
            setOnClickListener { d.dismiss() }
        }
        val scroll = ScrollView(this).apply {
            addView(image)
            setBackgroundColor(bmp.getPixel(0, bmp.height - 1))
            isVerticalScrollBarEnabled = false
        }
        val maxH = (root.height * 0.6f).toInt()
        val frame = FrameLayout(this).apply {
            setBackgroundColor(Color.argb(if (night) 90 else 50, 0, 0, 0))
            val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, minOf(bmp.height, maxH), Gravity.BOTTOM)
            addView(scroll, lp)
            setOnClickListener { d.dismiss() }
        }
        d.setContentView(frame)
        d.window?.apply {
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
            attributes = attributes.apply {
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            }
            setDecorFitsSystemWindows(false)
        }
        d.show()
        d.window?.insetsController?.hide(WindowInsets.Type.systemBars())
    }

    // ---- Свайпы по краям: размер текста и яркость ----

    private enum class Edge { LEFT, RIGHT }

    private var edge: Edge? = null
    private var edgeActive = false
    private var edgeY = 0f
    private var edgeX = 0f
    private var edgeStartStep = 0
    private var edgeStartBrightness = 0.5f
    private var edgeStep = 0
    private val touchSlop by lazy { ViewConfiguration.get(this).scaledTouchSlop }
    private val hideHud = Runnable { hud.visibility = View.GONE }

    /**
     * Касание у края проходит к странице как обычно (листание, меню). Если палец
     * пошёл по вертикали — это свайп края: странице посылается отмена, дальше жест наш.
     */
    override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val zone = root.width * EDGE_ZONE
                edge = when {
                    book == null || chromeShown -> null
                    ev.x < zone -> Edge.LEFT
                    ev.x > root.width - zone -> Edge.RIGHT
                    else -> null
                }
                edgeActive = false
                edgeX = ev.x
                edgeY = ev.y
            }
            MotionEvent.ACTION_MOVE -> {
                val e = edge
                if (e != null && !edgeActive) {
                    val dy = ev.y - edgeY
                    if (kotlin.math.abs(dy) > touchSlop * 2 && kotlin.math.abs(dy) > kotlin.math.abs(ev.x - edgeX) * 1.5f) {
                        edgeActive = true
                        edgeY = ev.y
                        edgeStartStep = settings.step
                        edgeStep = settings.step
                        edgeStartBrightness = settings.brightnessOrSystem()
                        val cancel = MotionEvent.obtain(ev).apply { action = MotionEvent.ACTION_CANCEL }
                        super.dispatchTouchEvent(cancel)
                        cancel.recycle()
                    }
                }
                if (edgeActive) {
                    edgeMove(ev.y - edgeY)
                    return true
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (edgeActive) {
                    edgeActive = false
                    edgeDone()
                    return true
                }
                edge = null
            }
        }
        return super.dispatchTouchEvent(ev)
    }

    /** Вверх — больше, вниз — меньше. Ступень размера — на каждые 6 % высоты экрана. */
    private fun edgeMove(dy: Float) {
        val h = root.height.toFloat()
        when (edge) {
            Edge.LEFT -> {
                val steps = (-dy / (h * 0.06f)).toInt()
                edgeStep = (edgeStartStep + steps).coerceIn(0, settings.steps.size - 1)
                showHud("Размер ${edgeStep + 1} из ${settings.steps.size}")
            }
            Edge.RIGHT -> {
                // Шкала квадратичная: внизу шаги мельче — там глаз различает больше.
                val start = kotlin.math.sqrt(edgeStartBrightness)
                val v = (start - dy / (h * 0.6f)).coerceIn(0.03f, 1f)
                val b = v * v
                settings.brightness = b
                applyBrightness()
                showHud("Яркость ${(b * 100).toInt().coerceAtLeast(1)} %")
            }
            null -> {}
        }
    }

    private fun edgeDone() {
        if (edge == Edge.LEFT && edgeStep != settings.step) {
            settings.step = edgeStep
            book?.let { open(it) }
        }
        edge = null
        main.removeCallbacks(hideHud)
        main.postDelayed(hideHud, 700)
    }

    private fun showHud(text: String) {
        hud.text = text
        hud.visibility = View.VISIBLE
        main.removeCallbacks(hideHud)
    }

    /** Своя яркость окна поверх системной (и её автонастройки). */
    private fun applyBrightness() {
        val b = settings.brightness
        if (b < 0f) return
        window.attributes = window.attributes.apply { screenBrightness = b.coerceIn(0.01f, 1f) }
    }

    // ---- Тема и экран ----

    override fun onConfigurationChanged(c: Configuration) {
        super.onConfigurationChanged(c)
        val dark = isNight()
        if (dark == night) return
        night = dark
        val bg = getColor(R.color.page)
        window.setBackgroundDrawable(ColorDrawable(bg))
        stripView.background = bg
        hud.setTextColor(getColor(R.color.ink))
        (hud.background as? android.graphics.drawable.GradientDrawable)?.setColor(getColor(R.color.panel))
        buildChrome()
        updateSystemBars()
        gen++
        clearImages()
        if (count == 0) return
        if (scrollMode) {
            stripView.invalidate()
            onSettled(stripView.chapter, stripView.top)
        } else {
            request(page)
            prefetch()
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) updateSystemBars()
    }

    /** Вернулись из настроек: яркость — сразу, шрифт, размер и листание — переверстать книгу. */
    override fun onResume() {
        super.onResume()
        applyBrightness()
        val b = book
        if (started && b != null && settings.layoutKey != openedKey) open(b)
    }

    /** Строка состояния видна вместе с меню; книга всегда остаётся во весь экран. */
    private fun updateSystemBars() {
        window.setDecorFitsSystemWindows(false)
        window.insetsController?.apply {
            systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
            setSystemBarsAppearance(if (night) 0 else light, light)
            hide(WindowInsets.Type.navigationBars())
            if (chromeShown) show(WindowInsets.Type.statusBars()) else hide(WindowInsets.Type.statusBars())
        }
    }

    private fun isNight() =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    private fun fg() = getColor(R.color.ink)

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_LONG).show()

    override fun onDestroy() {
        super.onDestroy()
        worker.execute {
            if (handle != 0L) Core.close(handle)
            handle = 0
        }
        worker.shutdown()
    }

    companion object {
        /** Имя файла книги в папке библиотеки: так экран библиотеки говорит, что открыть. */
        const val EXTRA_BOOK = "book"
        private const val TAG = "BookWorm"
        private const val REPORT_DOCUMENT = 901
        /** Плитка прокрутки — полэкрана, ~5 МБ; держим около трёх экранов. */
        private const val MAX_TILES = 8
        /** Ширина полосы у края экрана для свайпов размера и яркости. */
        private const val EDGE_ZONE = 0.1f
    }
}
