package app.bookworm

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import android.widget.CheckBox
import java.io.File
import java.util.concurrent.Executors

/**
 * Стартовый экран: сетка обложек, как «Книги» в Apple и Kindle. Последняя открытая — первой,
 * под обложкой — сколько прочитано или «Новая». Касание открывает книгу на месте остановки,
 * долгое — действия (удалить). Вверху «+» (добавить файл) и шестерёнка (настройки);
 * большой заголовок при прокрутке уходит в строку навигации.
 */
class LibraryActivity : ThemedActivity() {
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var books: Books
    private lateinit var scroll: ScrollView
    private lateinit var content: LinearLayout
    private lateinit var grid: LinearLayout
    private lateinit var titleView: TextView
    private lateinit var bar: FrameLayout
    private lateinit var barTitle: TextView
    private lateinit var barLine: View
    private var ready = false
    private var reading = false
    private var topInset = 0
    private var bottomInset = 0

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        books = Books(this)
        window.setDecorFitsSystemWindows(false)

        val accent = res(R.color.accent)
        titleView = label(this, "Библиотека", 34f, res(R.color.ink), 700).apply { letterSpacing = -0.01f }
        grid = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), 0, dp(16), 0)
            addView(titleView, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(20) })
            addView(CheckBox(this@LibraryActivity).apply {
                text = "Скрыть прочитанные"
                setTextColor(res(R.color.ink))
                buttonTintList = android.content.res.ColorStateList.valueOf(accent)
                isChecked = books.hideFinished
                minHeight = dp(48)
                setOnCheckedChangeListener { _, checked -> books.hideFinished = checked; if (ready) reload() }
            }, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })
            addView(grid, LinearLayout.LayoutParams(-1, -2))
        }
        scroll = ScrollView(this).apply {
            addView(content)
            isVerticalScrollBarEnabled = false
            clipToPadding = false
        }

        barTitle = label(this, "Библиотека", 17f, res(R.color.ink), 600).apply { alpha = 0f }
        barLine = View(this).apply {
            setBackgroundColor(res(R.color.separator))
            alpha = 0f
        }
        val buttons = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(iconButton(this@LibraryActivity, Icon.Kind.GEAR, "Настройки", accent) {
                startActivity(Intent(this@LibraryActivity, SettingsActivity::class.java))
            })
            addView(iconButton(this@LibraryActivity, Icon.Kind.PLUS, "Добавить книгу", accent) { pickBooks() })
        }
        bar = FrameLayout(this).apply {
            addView(barTitle, FrameLayout.LayoutParams(-2, -2, Gravity.CENTER_HORIZONTAL or Gravity.BOTTOM).apply { bottomMargin = dp(13) })
            addView(buttons, FrameLayout.LayoutParams(-2, dp(48), Gravity.END or Gravity.BOTTOM).apply { rightMargin = dp(4) })
            addView(barLine, FrameLayout.LayoutParams(-1, 1, Gravity.BOTTOM))
        }

        val root = FrameLayout(this).apply {
            setBackgroundColor(res(R.color.page))
            addView(scroll, FrameLayout.LayoutParams(-1, -1))
            addView(bar, FrameLayout.LayoutParams(-1, dp(52), Gravity.TOP))
        }
        setContentView(root)
        root.onInsets { top, bottom ->
            topInset = top
            bottomInset = bottom
            bar.layoutParams = (bar.layoutParams as FrameLayout.LayoutParams).apply { height = top + dp(52) }
            content.setPadding(dp(16), top + dp(52), dp(16), bottom + dp(32))
        }
        scroll.setOnScrollChangeListener { _, _, y, _, _ ->
            // Большой заголовок уехал под строку навигации — в ней появляется маленький, под ней линия.
            val from = titleView.top.toFloat() - dp(8)
            val t = ((y - from) / dp(34f)).coerceIn(0f, 1f)
            barTitle.alpha = t
            barLine.alpha = t
            bar.setBackgroundColor((res(R.color.page) and 0x00FFFFFF) or ((t * 235).toInt() shl 24))
        }

        worker.execute {
            books.prepare()
            main.post {
                ready = true
                reload()
            }
        }
    }

    override fun onResume() {
        super.onResume()
        if (ready) reload()
    }

    private fun reload() {
        val all = books.list()
        val files = all.filter { !books.hideFinished || !books.isFinished(it) }
        // Названия и авторов из самих файлов берём в фоне один раз; потом обложки перерисуются.
        val unknown = files.filter { books.needsFileInfo(it) }
        if (unknown.isNotEmpty() && !reading) {
            reading = true
            worker.execute {
                unknown.forEach { runCatching { books.fileInfo(it) } }
                main.post {
                    reading = false
                    if (!isDestroyed) reload()
                }
            }
        }
        grid.removeAllViews()
        if (files.isEmpty()) {
            if (all.isEmpty()) grid.addView(emptyState())
            else grid.addView(label(this, "Все книги прочитаны. Снимите фильтр, чтобы увидеть их.", 17f, res(R.color.muted)))
            return
        }
        val gap = dp(16)
        val widthPx = resources.displayMetrics.widthPixels - dp(32)
        val cols = maxOf(3, (widthPx / dpf(120)).toInt())
        val cellW = (widthPx - gap * (cols - 1)) / cols
        for (chunk in files.chunked(cols)) {
            val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            chunk.forEachIndexed { i, f ->
                row.addView(cell(f), LinearLayout.LayoutParams(cellW, -2).apply { if (i < cols - 1) rightMargin = gap })
            }
            grid.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(24) })
        }
    }

    private fun cell(f: File): View {
        val info = books.info(f)
        val title = info.title
        val pct = books.progress(f)
        val finished = books.isFinished(f)
        val status = (if (pct < 0) "Новая" else "$pct %") + if (finished) " ⚑" else ""
        return LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            isClickable = true
            contentDescription = "$title, ${if (finished) "книга прочитана" else if (pct < 0) "новая" else "прочитано $pct %"}"
            addView(
                CoverView(this@LibraryActivity, title, info.author, books.coverFile(f).takeIf { it.exists() }?.let { android.graphics.BitmapFactory.decodeFile(it.path) }),
                LinearLayout.LayoutParams(-1, -2),
            )
            addView(
                label(this@LibraryActivity, status, 12f, if (pct < 0) res(R.color.accent) else res(R.color.muted), if (pct < 0) 600 else 400),
                LinearLayout.LayoutParams(-2, -2).apply { topMargin = dp(8) },
            )
            setOnClickListener { open(f) }
            setOnLongClickListener {
                tick()
                actions(f, title)
                true
            }
            pressScale(0.95f)
        }
    }

    private fun emptyState(): View = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        setPadding(dp(8), dp(96), dp(8), 0)
        addView(label(this@LibraryActivity, "В библиотеке пусто", 22f, res(R.color.ink), 600))
        addView(
            label(this@LibraryActivity, "Добавьте книгу FB2 или EPUB кнопкой «+» или через «Поделиться» в файловом менеджере.", 15f, res(R.color.muted)).apply {
                gravity = Gravity.CENTER
                setLineSpacing(0f, 1.15f)
            },
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(8) },
        )
        addView(
            label(this@LibraryActivity, "Добавить книгу", 17f, 0xFFFFFFFF.toInt(), 600).apply {
                gravity = Gravity.CENTER
                background = rounded(res(R.color.accent), dpf(12))
                isClickable = true
                setOnClickListener { pickBooks() }
                pressScale(0.97f)
            },
            LinearLayout.LayoutParams(dp(220), dp(50)).apply { topMargin = dp(28) },
        )
    }

    private fun open(f: File) {
        startActivity(Intent(this, ReaderActivity::class.java).putExtra(ReaderActivity.EXTRA_BOOK, f.name))
        overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, android.R.anim.fade_in, android.R.anim.fade_out)
    }

    private fun actions(f: File, title: String) {
        Sheet(this, title)
            .action("Сведения о книге") {
                startActivity(Intent(this, BookInfoActivity::class.java).putExtra(ReaderActivity.EXTRA_BOOK, f.name))
            }
            .action("Удалить", destructive = true) { confirmDelete(f, title) }
            .show()
    }

    private fun confirmDelete(f: File, title: String) {
        Sheet(this, "Удалить «$title»? Книга исчезнет только с этого телефона.")
            .action("Удалить", destructive = true) {
                books.delete(f)
                reload()
            }
            .show()
    }

    private fun pickBooks() {
        val pick = Intent(Intent.ACTION_OPEN_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .setType("*/*")
            .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        @Suppress("DEPRECATION")
        startActivityForResult(pick, PICK_BOOKS)
    }

    @Deprecated("Простой Activity без AndroidX")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != PICK_BOOKS || resultCode != RESULT_OK || data == null) return
        val uris = buildList {
            data.data?.let { add(it) }
            data.clipData?.let { c -> for (i in 0 until c.itemCount) add(c.getItemAt(i).uri) }
        }.distinct()
        worker.execute {
            val done = uris.map { runCatching { books.import(it) } }
            main.post {
                done.firstOrNull { it.isFailure }?.exceptionOrNull()?.let {
                    Toast.makeText(this, "Не удалось добавить книгу: ${it.message}", Toast.LENGTH_LONG).show()
                }
                val added = done.mapNotNull { it.getOrNull() }
                reload()
                // Одну выбранную книгу читать и хочется: открываем сразу.
                if (added.size == 1) open(added[0])
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        worker.shutdown()
    }

    companion object {
        private const val PICK_BOOKS = 1
    }
}
