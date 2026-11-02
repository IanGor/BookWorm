package app.bookworm

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import android.widget.CheckBox
import java.io.File
import java.util.concurrent.Executors

/**
 * Сведения о книге: название и автор правятся руками или подставляются из сети («Найти в сети»
 * ищет по тому, что сейчас в полях; выбранный вариант заполняет поля и скачивает обложку).
 * Пока не нажато «Готово», ничего не сохраняется.
 */
class BookInfoActivity : ThemedActivity() {
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var books: Books
    private lateinit var book: File
    private lateinit var titleField: EditText
    private lateinit var authorField: EditText
    private lateinit var coverBox: FrameLayout
    private lateinit var results: LinearLayout
    private lateinit var searchButton: TextView
    private lateinit var scroll: ScrollView
    private lateinit var content: LinearLayout
    private lateinit var finishedField: CheckBox

    /** Обложка, выбранная в поиске: сохранится по «Готово». */
    private var newCover: ByteArray? = null
    private var dropCover = false
    private var searching = false

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        books = Books(this)
        val name = intent.getStringExtra(ReaderActivity.EXTRA_BOOK)
        book = File(books.dir, name ?: "").takeIf { name != null && it.exists() } ?: return finish()
        window.setDecorFitsSystemWindows(false)

        titleField = field("Название", InputType.TYPE_TEXT_FLAG_CAP_SENTENCES)
        authorField = field("Автор", InputType.TYPE_TEXT_FLAG_CAP_WORDS)
        finishedField = CheckBox(this).apply {
            text = "Прочитано"
            setTextColor(res(R.color.ink))
            buttonTintList = android.content.res.ColorStateList.valueOf(res(R.color.accent))
            isChecked = state?.getBoolean("finished") ?: books.isFinished(book)
            minHeight = dp(48)
        }
        coverBox = FrameLayout(this)
        results = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        searchButton = label(this, "Найти в сети", 17f, 0xFFFFFFFF.toInt(), 600).apply {
            gravity = Gravity.CENTER
            background = rounded(res(R.color.accent), dpf(12))
            isClickable = true
            setOnClickListener { search() }
            pressScale(0.98f)
        }
        titleField.setOnEditorActionListener { _, a, _ -> (a == EditorInfo.IME_ACTION_NEXT).also { if (it) authorField.requestFocus() } }
        authorField.setOnEditorActionListener { _, a, _ -> (a == EditorInfo.IME_ACTION_SEARCH).also { if (it) search() } }

        content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), 0, dp(16), 0)
            addView(coverBox, LinearLayout.LayoutParams(dp(132), -2).apply { gravity = Gravity.CENTER_HORIZONTAL; topMargin = dp(12) })
            addView(header("Книга"))
            addView(card(titleField, authorField))
            addView(finishedField, LinearLayout.LayoutParams(-1, -2))
            addView(searchButton, LinearLayout.LayoutParams(-1, dp(50)).apply { topMargin = dp(16) })
            addView(results, LinearLayout.LayoutParams(-1, -2))
            addView(footer("Поиск идёт по названию и автору из полей выше — в сеть (Google Books, затем Open Library) уходят только они. Выбранный вариант подставляет название, автора и обложку; их можно поправить руками."))
            addView(
                label(this@BookInfoActivity, "Вернуть данные из файла книги", 17f, res(R.color.accent)).apply {
                    gravity = Gravity.CENTER
                    isClickable = true
                    setOnClickListener { resetToFile() }
                    pressDim()
                },
                LinearLayout.LayoutParams(-1, dp(52)).apply { topMargin = dp(20) },
            )
        }
        scroll = ScrollView(this).apply {
            addView(content)
            isVerticalScrollBarEnabled = false
            clipToPadding = false
            isFillViewport = true
        }
        val bar = FrameLayout(this).apply {
            addView(
                label(this@BookInfoActivity, "Отмена", 17f, res(R.color.accent)).apply {
                    gravity = Gravity.CENTER
                    setPadding(dp(16), 0, dp(16), 0)
                    isClickable = true
                    setOnClickListener { finish() }
                    pressDim()
                },
                FrameLayout.LayoutParams(-2, dp(48), Gravity.START or Gravity.BOTTOM),
            )
            addView(label(this@BookInfoActivity, "Сведения", 17f, res(R.color.ink), 600), FrameLayout.LayoutParams(-2, -2, Gravity.CENTER_HORIZONTAL or Gravity.BOTTOM).apply { bottomMargin = dp(13) })
            addView(
                label(this@BookInfoActivity, "Готово", 17f, res(R.color.accent), 600).apply {
                    gravity = Gravity.CENTER
                    setPadding(dp(16), 0, dp(16), 0)
                    isClickable = true
                    setOnClickListener { save() }
                    pressDim()
                },
                FrameLayout.LayoutParams(-2, dp(48), Gravity.END or Gravity.BOTTOM),
            )
        }
        val root = FrameLayout(this).apply {
            setBackgroundColor(res(R.color.group))
            addView(scroll, FrameLayout.LayoutParams(-1, -1))
            addView(bar, FrameLayout.LayoutParams(-1, dp(52), Gravity.TOP))
        }
        setContentView(root)
        root.setOnApplyWindowInsetsListener { _, ins ->
            val sys = ins.getInsetsIgnoringVisibility(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            val ime = ins.getInsets(WindowInsets.Type.ime()).bottom
            bar.layoutParams = (bar.layoutParams as FrameLayout.LayoutParams).apply { height = sys.top + dp(52) }
            content.setPadding(dp(16), sys.top + dp(52), dp(16), maxOf(sys.bottom, ime) + dp(32))
            ins
        }
        root.requestApplyInsets()

        val i = books.info(book)
        titleField.setText(i.title)
        authorField.setText(i.author)
        showCover()
        // Если книгу ещё не разбирали, подставятся данные из файла.
        if (books.needsFileInfo(book)) worker.execute {
            val f = runCatching { books.fileInfo(book) }.getOrNull() ?: return@execute
            main.post {
                if (titleField.text.toString() == i.title && authorField.text.toString() == i.author) {
                    titleField.setText(f.title)
                    authorField.setText(f.author)
                    showCover()
                }
            }
        }
    }

    // ---- Обложка ----

    private fun showCover() {
        val bmp: Bitmap? = newCover?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
            ?: books.coverFile(book).takeIf { !dropCover && it.exists() }?.let { BitmapFactory.decodeFile(it.path) }
        coverBox.removeAllViews()
        coverBox.addView(CoverView(this, titleField.text.toString().ifBlank { Books.cleanTitle(book) }, authorField.text.toString(), bmp), FrameLayout.LayoutParams(-1, -2))
    }

    // ---- Поиск ----

    private fun search() {
        if (searching) return
        val t = titleField.text.toString().trim()
        val a = authorField.text.toString().trim()
        if (t.isEmpty() && a.isEmpty()) return toast("Введите название или автора")
        searching = true
        searchButton.text = "Ищу…"
        searchButton.alpha = 0.6f
        results.removeAllViews()
        currentFocus?.let { (getSystemService(android.view.inputmethod.InputMethodManager::class.java)).hideSoftInputFromWindow(it.windowToken, 0) }
        worker.execute {
            val r = runCatching { BookSearch.search(t, a) }
            main.post {
                searching = false
                searchButton.text = "Найти в сети"
                searchButton.alpha = 1f
                r.onSuccess { hits ->
                    if (hits.isEmpty()) showMessage("Ничего не нашлось. Попробуйте изменить название или убрать автора.")
                    else showResults(hits)
                }.onFailure { showMessage("Нет связи или сервис не отвечает. Проверьте интернет и попробуйте ещё раз.") }
            }
        }
    }

    private fun showMessage(s: String) {
        results.removeAllViews()
        results.addView(label(this, s, 15f, res(R.color.muted)).apply {
            setPadding(dp(16), dp(16), dp(16), 0)
            setLineSpacing(0f, 1.1f)
        })
    }

    private fun showResults(hits: List<BookSearch.Hit>) {
        results.removeAllViews()
        results.addView(header("Найдено — выберите свою"))
        results.addView(card(*hits.map { hitRow(it) }.toTypedArray()))
    }

    private fun hitRow(h: BookSearch.Hit): View = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        minimumHeight = dp(56)
        setPadding(dp(16), dp(10), dp(16), dp(10))
        isClickable = true
        addView(label(this@BookInfoActivity, h.title, 17f, res(R.color.ink)).apply { maxLines = 2; ellipsize = android.text.TextUtils.TruncateAt.END })
        val sub = listOf(h.author, h.year).filter { it.isNotBlank() }.joinToString(" · ")
        if (sub.isNotBlank()) addView(label(this@BookInfoActivity, sub, 14f, res(R.color.muted)).apply { maxLines = 2; ellipsize = android.text.TextUtils.TruncateAt.END })
        setOnTouchListener { v, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> v.setBackgroundColor(res(R.color.group))
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> v.background = null
            }
            false
        }
        setOnClickListener { pick(h) }
    }

    private fun pick(h: BookSearch.Hit) {
        searchButton.tick()
        titleField.setText(h.title)
        authorField.setText(h.author)
        results.removeAllViews()
        newCover = null
        dropCover = true
        showCover()
        val url = h.coverUrl ?: return
        worker.execute {
            val bytes = BookSearch.cover(url)
            main.post {
                if (bytes != null) {
                    newCover = bytes
                    showCover()
                } else toast("Название и автор подставлены, обложка не скачалась")
            }
        }
    }

    // ---- Сохранение ----

    private fun resetToFile() {
        worker.execute {
            val f = runCatching { books.fileInfo(book) }.getOrNull() ?: Books.Info(Books.cleanTitle(book), "")
            main.post {
                titleField.setText(f.title)
                authorField.setText(f.author)
                newCover = null
                dropCover = true
                showCover()
            }
        }
    }

    private fun save() {
        val t = titleField.text.toString().trim().ifEmpty { Books.cleanTitle(book) }
        val a = authorField.text.toString().trim()
        val cover = newCover
        val finished = finishedField.isChecked
        worker.execute {
            if (cover != null) books.saveCover(book, cover) else if (dropCover) books.coverFile(book).delete()
            books.setInfo(book, t, a)
            books.setFinished(book, finished)
            main.post { finish() }
        }
    }

    override fun onSaveInstanceState(out: Bundle) {
        out.putBoolean("finished", finishedField.isChecked)
        super.onSaveInstanceState(out)
    }

    // ---- Элементы ----

    private fun field(hint: String, caps: Int): EditText = EditText(this).apply {
        this.hint = hint
        setHintTextColor(res(R.color.muted))
        setTextColor(res(R.color.ink))
        textSize = 17f
        typeface = weight(400)
        background = null
        setSingleLine()
        inputType = InputType.TYPE_CLASS_TEXT or caps
        imeOptions = if (hint == "Автор") EditorInfo.IME_ACTION_SEARCH else EditorInfo.IME_ACTION_NEXT
        setPadding(dp(16), dp(15), dp(16), dp(15))
        minHeight = dp(52)
    }

    private fun header(s: String): View =
        label(this, s.uppercase(), 13f, res(R.color.muted)).apply {
            letterSpacing = 0.02f
            setPadding(dp(16), 0, dp(16), dp(6))
            layoutParams = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(24) }
        }

    private fun footer(s: String): View =
        label(this, s, 13f, res(R.color.muted)).apply {
            setLineSpacing(0f, 1.12f)
            setPadding(dp(16), dp(12), dp(16), 0)
        }

    private fun card(vararg rows: View): View = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = rounded(res(R.color.cell), dpf(12))
        clipToOutline = true
        rows.forEachIndexed { i, r ->
            addView(r, LinearLayout.LayoutParams(-1, -2))
            if (i < rows.size - 1) {
                addView(View(this@BookInfoActivity).apply { setBackgroundColor(res(R.color.separator)) }, LinearLayout.LayoutParams(-1, 1).apply { leftMargin = dp(16) })
            }
        }
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_LONG).show()

    override fun onDestroy() {
        super.onDestroy()
        worker.shutdown()
    }
}
