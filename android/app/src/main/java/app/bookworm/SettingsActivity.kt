package app.bookworm

import android.app.Activity
import android.content.Intent
import android.content.res.ColorStateList
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import android.widget.Toast
import java.util.concurrent.Executors

/**
 * Настройки: сгруппированные списки, как в iOS. Заголовок группы — над ней, пояснение — под ней.
 * Всё меняется сразу, кнопки «Сохранить» нет; «Готово» просто закрывает экран.
 * Читалка при возвращении сама переверстает книгу, если изменились шрифт, размер или листание.
 */
class SettingsActivity : ThemedActivity() {
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var settings: Settings
    private lateinit var scroll: ScrollView
    private var fams: List<Settings.Family> = emptyList()
    private lateinit var fontValue: TextView
    private lateinit var ownValue: TextView

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        settings = Settings(this)
        window.setDecorFitsSystemWindows(false)

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), 0, dp(16), 0)
        }

        // Текст: гарнитура и размер.
        fontValue = label(this, "…", 17f, res(R.color.muted))
        content.addView(header("Текст"))
        content.addView(card(
            navRow("Шрифт", fontValue) { chooseFont() },
            sliderRow(
                small = label(this, "A", 13f, res(R.color.ink), 500),
                large = label(this, "A", 24f, res(R.color.ink), 500),
                max = settings.steps.size - 1,
                value = settings.step,
            ) { settings.step = it },
        ))
        content.addView(footer("Размер текста меняется и вертикальным свайпом по левому краю экрана при чтении."))

        content.addView(header("Бионическое чтение"))
        content.addView(card(controlRow(Segmented(this, listOf("Выключено", "Включено"), if (settings.bionic) 1 else 0) {
            settings.bionic = it == 1
        })))
        content.addView(footer("Начальные буквы слов выделяются более плотным начертанием."))

        // Оформление: тема.
        content.addView(header("Оформление"))
        val themes = listOf("auto", "day", "night")
        content.addView(card(controlRow(Segmented(this, listOf("Авто", "День", "Ночь"), themes.indexOf(settings.theme).coerceAtLeast(0)) {
            settings.theme = themes[it]
            recreate()
        })))
        content.addView(footer("«Авто» повторяет тему системы."))

        // Листание.
        content.addView(header("Листание"))
        content.addView(card(controlRow(Segmented(this, listOf("Страницы", "Прокрутка"), if (settings.scroll) 1 else 0) {
            settings.scroll = it == 1
        })))
        content.addView(footer("Касание в верхней трети экрана — назад, в нижней — вперёд, в центре — меню. В режиме прокрутки текст можно двигать пальцем."))

        // Яркость.
        content.addView(header("Яркость"))
        content.addView(card(sliderRow(
            small = ImageView(this).apply { setImageDrawable(Icon(Icon.Kind.SUN_SMALL, res(R.color.muted))) },
            large = ImageView(this).apply { setImageDrawable(Icon(Icon.Kind.SUN, res(R.color.muted))) },
            max = 100,
            value = (Math.sqrt(settings.brightnessOrSystem().toDouble()) * 100).toInt().coerceIn(3, 100),
            ticks = false,
        ) {
            // Шкала квадратичная: внизу шаги мельче — там глаз различает больше.
            val v = it.coerceAtLeast(3) / 100f
            settings.brightness = v * v
            window.attributes = window.attributes.apply { screenBrightness = v * v }
        }))
        content.addView(footer("Яркость только в BookWorm: системная и автояркость на неё не влияют. Меняется и вертикальным свайпом по правому краю экрана."))

        // Свои шрифты.
        ownValue = label(this, "", 17f, res(R.color.muted))
        content.addView(header("Шрифты"))
        content.addView(card(navRow("Свои шрифты", ownValue) { pickFontFolder() }))
        content.addView(footer("Выберите папку с файлами TTF или OTF — гарнитуры появятся в списке шрифтов с пометкой «свой». Папку Download целиком Android выбрать не даёт, только вложенную."))

        scroll = ScrollView(this).apply {
            addView(content)
            isVerticalScrollBarEnabled = false
            clipToPadding = false
        }
        val barLine = View(this).apply {
            setBackgroundColor(res(R.color.separator))
            alpha = 0f
        }
        val bar = FrameLayout(this).apply {
            addView(label(this@SettingsActivity, "Настройки", 17f, res(R.color.ink), 600), FrameLayout.LayoutParams(-2, -2, Gravity.CENTER_HORIZONTAL or Gravity.BOTTOM).apply { bottomMargin = dp(13) })
            addView(
                label(this@SettingsActivity, "Готово", 17f, res(R.color.accent), 600).apply {
                    gravity = Gravity.CENTER
                    setPadding(dp(16), 0, dp(16), 0)
                    isClickable = true
                    setOnClickListener { finish() }
                    pressDim()
                },
                FrameLayout.LayoutParams(-2, dp(48), Gravity.END or Gravity.BOTTOM),
            )
            addView(barLine, FrameLayout.LayoutParams(-1, 1, Gravity.BOTTOM))
        }
        val root = FrameLayout(this).apply {
            setBackgroundColor(res(R.color.group))
            addView(scroll, FrameLayout.LayoutParams(-1, -1))
            addView(bar, FrameLayout.LayoutParams(-1, dp(52), Gravity.TOP))
        }
        setContentView(root)
        root.onInsets { top, bottom ->
            bar.layoutParams = (bar.layoutParams as FrameLayout.LayoutParams).apply { height = top + dp(52) }
            content.setPadding(dp(16), top + dp(52), dp(16), bottom + dp(32))
        }
        scroll.setOnScrollChangeListener { _, _, y, _, _ ->
            val t = (y / dpf(12)).coerceIn(0f, 1f)
            barLine.alpha = t
            bar.setBackgroundColor((res(R.color.group) and 0x00FFFFFF) or ((t * 235).toInt() shl 24))
        }
        state?.getInt("scroll")?.let { y -> scroll.post { scroll.scrollTo(0, y) } }

        loadFamilies()
    }

    override fun onSaveInstanceState(out: Bundle) {
        super.onSaveInstanceState(out)
        out.putInt("scroll", scroll.scrollY)
    }

    // ---- Строки и группы ----

    private fun header(s: String): View =
        label(this, s.uppercase(), 13f, res(R.color.muted)).apply {
            letterSpacing = 0.02f
            setPadding(dp(16), 0, dp(16), dp(6))
            layoutParams = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(28) }
        }

    private fun footer(s: String): View =
        label(this, s, 13f, res(R.color.muted)).apply {
            setLineSpacing(0f, 1.12f)
            setPadding(dp(16), dp(8), dp(16), 0)
        }

    /** Группа: скруглённая плашка, строки разделены линией с отступом слева. */
    private fun card(vararg rows: View): View = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = rounded(res(R.color.cell), dpf(12))
        clipToOutline = true
        rows.forEachIndexed { i, r ->
            addView(r, LinearLayout.LayoutParams(-1, -2))
            if (i < rows.size - 1) {
                addView(View(this@SettingsActivity).apply { setBackgroundColor(res(R.color.separator)) }, LinearLayout.LayoutParams(-1, 1).apply { leftMargin = dp(16) })
            }
        }
    }

    /** Строка-переход: название слева, текущее значение и шеврон справа. */
    private fun navRow(title: String, value: TextView, onClick: () -> Unit): View = FrameLayout(this).apply {
        minimumHeight = dp(52)
        isClickable = true
        setOnClickListener { onClick() }
        setOnTouchListener { v, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> v.setBackgroundColor(res(R.color.group))
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> v.background = null
            }
            false
        }
        addView(label(this@SettingsActivity, title, 17f, res(R.color.ink)), FrameLayout.LayoutParams(-2, -2, Gravity.CENTER_VERTICAL).apply {
            leftMargin = dp(16)
            topMargin = dp(15)
            bottomMargin = dp(15)
        })
        value.maxLines = 1
        value.ellipsize = android.text.TextUtils.TruncateAt.END
        value.maxWidth = (resources.displayMetrics.widthPixels * 0.5f).toInt()
        addView(LinearLayout(this@SettingsActivity).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(value)
            addView(ImageView(this@SettingsActivity).apply {
                setImageDrawable(Icon(Icon.Kind.BACK, res(R.color.muted)))
                scaleX = -1f
                alpha = 0.6f
            }, LinearLayout.LayoutParams(dp(18), dp(18)).apply { leftMargin = dp(4) })
        }, FrameLayout.LayoutParams(-2, -2, Gravity.END or Gravity.CENTER_VERTICAL).apply { rightMargin = dp(12) })
    }

    private fun controlRow(c: View): View = FrameLayout(this).apply {
        addView(c, FrameLayout.LayoutParams(-1, -2).apply { setMargins(dp(12), dp(10), dp(12), dp(10)) })
    }

    /** Ползунок с подписями по краям — «меньше» и «больше». */
    private fun sliderRow(small: View, large: View, max: Int, value: Int, ticks: Boolean = true, onChange: (Int) -> Unit): View =
        LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(56)
            setPadding(dp(16), 0, dp(16), 0)
            addView(small, LinearLayout.LayoutParams(dp(24), -2).apply { if (small is ImageView) height = dp(24) }.also { (small as? TextView)?.gravity = Gravity.CENTER })
            addView(SeekBar(this@SettingsActivity).apply {
                this.max = max
                progress = value
                val accent = ColorStateList.valueOf(res(R.color.accent))
                progressTintList = accent
                thumbTintList = accent
                progressBackgroundTintList = ColorStateList.valueOf((res(R.color.muted) and 0x00FFFFFF) or (0x55 shl 24))
                setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(s: SeekBar, p: Int, fromUser: Boolean) {
                        if (!fromUser) return
                        if (ticks) s.tick()
                        onChange(p)
                    }

                    override fun onStartTrackingTouch(s: SeekBar) = Unit
                    override fun onStopTrackingTouch(s: SeekBar) = Unit
                })
            }, LinearLayout.LayoutParams(0, dp(48), 1f))
            addView(large, LinearLayout.LayoutParams(dp(28), -2).apply { if (large is ImageView) height = dp(28) }.also { (large as? TextView)?.gravity = Gravity.CENTER })
        }

    // ---- Шрифты ----

    private fun loadFamilies() {
        worker.execute {
            val list = settings.families()
            main.post {
                fams = list
                fontValue.text = settings.current(list)?.name ?: "—"
                val own = list.filter { it.user }
                ownValue.text = if (own.isEmpty()) "Нет" else own.size.toString()
            }
        }
    }

    private fun chooseFont() {
        if (fams.isEmpty()) return
        val cur = settings.current(fams)
        val sheet = Sheet(this, "Шрифт")
        for (f in fams) {
            sheet.choice(f.label, f == cur) {
                if (!f.cyrillic) toast("В «${f.name}» нет русских букв: русский текст не отобразится")
                settings.family = f.name
                settings.familyIsUser = f.user
                fontValue.text = f.name
            }
        }
        sheet.show()
    }

    private fun pickFontFolder() {
        @Suppress("DEPRECATION")
        startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), PICK_FONTS)
    }

    @Deprecated("Простой Activity без AndroidX")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == PICK_FONTS && resultCode == RESULT_OK) data?.data?.let { importFonts(it) }
    }

    private fun importFonts(tree: Uri) {
        runCatching { contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        toast("Копирую шрифты…")
        worker.execute {
            val n = runCatching { settings.importFolder(tree) }
            val list = settings.families()
            main.post {
                fams = list
                val own = list.count { it.user }
                ownValue.text = if (own == 0) "Нет" else own.toString()
                n.onSuccess { k ->
                    toast(if (k == 0) "В папке нет файлов TTF или OTF" else "Шрифтов: $k, гарнитур: $own")
                    if (own > 0) chooseFont()
                }.onFailure { toast("Не удалось прочитать папку: ${it.message}") }
            }
        }
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_LONG).show()

    override fun onDestroy() {
        super.onDestroy()
        worker.shutdown()
    }

    companion object {
        private const val PICK_FONTS = 2
    }
}
