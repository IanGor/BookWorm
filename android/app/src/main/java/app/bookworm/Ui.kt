package app.bookworm

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorFilter
import android.graphics.LinearGradient
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.text.TextUtils
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.view.ViewTreeObserver
import android.view.WindowInsets
import android.view.animation.DecelerateInterpolator
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/*
 * Общие кирпичики интерфейса: размеры в dp, значки, нижний лист, сегментный переключатель
 * и обложка. Всё по HIG: цели касания не меньше 44 pt, отклик на касание сразу
 * (не после отпускания), системный шрифт, цвета из ресурсов (день и ночь).
 */

fun Context.dp(v: Number): Int = (v.toFloat() * resources.displayMetrics.density).toInt()

fun Context.dpf(v: Number): Float = v.toFloat() * resources.displayMetrics.density

fun Context.res(id: Int): Int = getColor(id)

/** Начертание системного шрифта с нужной насыщенностью (400 — обычное, 600 — полужирное). */
fun weight(w: Int): Typeface = Typeface.create(Typeface.DEFAULT, w, false)

fun rounded(color: Int, radius: Float): GradientDrawable =
    GradientDrawable().apply {
        setColor(color)
        cornerRadius = radius
    }

fun roundedTop(color: Int, radius: Float): GradientDrawable =
    GradientDrawable().apply {
        setColor(color)
        cornerRadii = floatArrayOf(radius, radius, radius, radius, 0f, 0f, 0f, 0f)
    }

fun label(ctx: Context, s: String, sp: Float, color: Int, w: Int = 400): TextView =
    TextView(ctx).apply {
        text = s
        textSize = sp
        setTextColor(color)
        typeface = weight(w)
    }

/** Нажатие видно сразу: вид слегка уменьшается на касании и плавно возвращается. */
fun View.pressScale(scale: Float = 0.96f) {
    setOnTouchListener { v, e ->
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> v.animate().scaleX(scale).scaleY(scale).setDuration(90)
                .setInterpolator(DecelerateInterpolator()).start()
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> v.animate().scaleX(1f).scaleY(1f).setDuration(220)
                .setInterpolator(DecelerateInterpolator(2f)).start()
        }
        false
    }
}

/** Кнопка без рамки: на касании тускнеет, как текстовые кнопки в iOS. */
fun View.pressDim() {
    setOnTouchListener { v, e ->
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> v.alpha = 0.35f
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> v.animate().alpha(1f).setDuration(200).start()
        }
        false
    }
}

fun View.tick() = performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)

/** Кнопка-значок 48×48 dp. */
fun iconButton(ctx: Context, kind: Icon.Kind, desc: String, color: Int, onClick: () -> Unit): ImageView =
    ImageView(ctx).apply {
        setImageDrawable(Icon(kind, color))
        contentDescription = desc
        val p = ctx.dp(12)
        setPadding(p, p, p, p)
        layoutParams = ViewGroup.LayoutParams(ctx.dp(48), ctx.dp(48))
        isClickable = true
        setOnClickListener { onClick() }
        pressDim()
    }

/** Системные отступы (вырез, строка состояния, жестовая панель): сверху и снизу в px. */
fun View.onInsets(apply: (top: Int, bottom: Int) -> Unit) {
    setOnApplyWindowInsetsListener { _, ins ->
        val b = ins.getInsetsIgnoringVisibility(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
        apply(b.top, b.bottom)
        ins
    }
    requestApplyInsets()
}

/** Значки в сетке 24×24, штрих со скруглёнными концами. */
class Icon(private val kind: Kind, color: Int) : Drawable() {
    enum class Kind { BACK, PLUS, GEAR, LIST, CHECK, SUN, SUN_SMALL }

    private val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        this.color = color
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color }

    private fun rays(c: Canvas, r0: Float, r1: Float) {
        for (k in 0 until 8) {
            val a = Math.toRadians(k * 45.0)
            val (x, y) = cos(a).toFloat() to sin(a).toFloat()
            c.drawLine(12f + r0 * x, 12f + r0 * y, 12f + r1 * x, 12f + r1 * y, p)
        }
    }

    override fun draw(c: Canvas) {
        val s = min(bounds.width(), bounds.height()) / 24f
        c.save()
        c.translate(bounds.left + (bounds.width() - 24 * s) / 2, bounds.top + (bounds.height() - 24 * s) / 2)
        c.scale(s, s)
        when (kind) {
            Kind.BACK -> {
                p.strokeWidth = 2.4f
                c.drawPath(Path().apply { moveTo(15f, 4.5f); lineTo(7.5f, 12f); lineTo(15f, 19.5f) }, p)
            }
            Kind.PLUS -> {
                p.strokeWidth = 2.2f
                c.drawLine(12f, 5f, 12f, 19f, p)
                c.drawLine(5f, 12f, 19f, 12f, p)
            }
            Kind.GEAR -> {
                p.strokeWidth = 1.9f
                c.drawCircle(12f, 12f, 3.2f, p)
                c.drawCircle(12f, 12f, 7.2f, p)
                p.strokeWidth = 2.8f
                rays(c, 7.2f, 10.4f)
            }
            Kind.LIST -> {
                p.strokeWidth = 2f
                for (y in listOf(7f, 12f, 17f)) {
                    c.drawLine(9.5f, y, 20f, y, p)
                    c.drawCircle(4.8f, y, 1.2f, fill)
                }
            }
            Kind.CHECK -> {
                p.strokeWidth = 2.4f
                c.drawPath(Path().apply { moveTo(4.5f, 12.5f); lineTo(9.5f, 17.5f); lineTo(19.5f, 6.5f) }, p)
            }
            Kind.SUN -> {
                p.strokeWidth = 2f
                c.drawCircle(12f, 12f, 4.2f, p)
                rays(c, 6.9f, 9.6f)
            }
            Kind.SUN_SMALL -> {
                p.strokeWidth = 1.8f
                c.drawCircle(12f, 12f, 2.6f, p)
                rays(c, 4.8f, 6.6f)
            }
        }
        c.restore()
    }

    override fun setAlpha(a: Int) {
        p.alpha = a
        fill.alpha = a
    }

    override fun setColorFilter(f: ColorFilter?) {
        p.colorFilter = f
        fill.colorFilter = f
    }

    @Deprecated("Deprecated in Java")
    override fun getOpacity() = PixelFormat.TRANSLUCENT
}

/**
 * Нижний лист. Два вида: список действий с отдельной «Отмена» (`action`) и выбор из списка
 * с галочкой (`choice`, лист высокий, как оглавление). Выезжает снизу и уходит туда же;
 * касание вне листа закрывает его.
 */
class Sheet(private val act: Activity, private val title: String? = null) {
    private class Row(val label: String, val destructive: Boolean, val checked: Boolean, val indent: Int, val onClick: () -> Unit)

    private val rows = ArrayList<Row>()
    private var choice = false

    fun action(label: String, destructive: Boolean = false, onClick: () -> Unit) = apply {
        rows += Row(label, destructive, false, 0, onClick)
    }

    fun choice(label: String, checked: Boolean, indent: Int = 0, onClick: () -> Unit) = apply {
        rows += Row(label, false, checked, indent, onClick)
        choice = true
    }

    fun show() {
        val d = Dialog(act, android.R.style.Theme_Translucent_NoTitleBar)
        val ink = act.res(R.color.ink)
        val muted = act.res(R.color.muted)
        val cell = act.res(R.color.cell)
        val sep = act.res(R.color.separator)
        val accent = act.res(R.color.accent)
        val danger = act.res(R.color.danger)
        val screenH = act.resources.displayMetrics.heightPixels

        val panel = LinearLayout(act).apply {
            orientation = LinearLayout.VERTICAL
            background = roundedTop(cell, act.dpf(14))
            isClickable = true
        }
        val close = { after: (() -> Unit)? ->
            panel.animate().translationY(panel.height.toFloat()).setDuration(220).setInterpolator(DecelerateInterpolator())
                .withEndAction {
                    d.dismiss()
                    after?.invoke()
                }.start()
        }

        val list = LinearLayout(act).apply { orientation = LinearLayout.VERTICAL }
        var checkedRow: View? = null
        rows.forEachIndexed { i, r ->
            val row = FrameLayout(act).apply {
                minimumHeight = act.dp(52)
                isClickable = true
                setOnClickListener { close(r.onClick) }
                setOnTouchListener { v, e ->
                    when (e.actionMasked) {
                        MotionEvent.ACTION_DOWN -> v.setBackgroundColor(act.res(R.color.group))
                        MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> v.background = null
                    }
                    false
                }
            }
            val t = label(act, r.label, 17f, if (r.destructive) danger else if (r.checked) accent else ink, if (r.checked) 600 else 400).apply {
                maxLines = 2
                ellipsize = TextUtils.TruncateAt.END
            }
            row.addView(t, FrameLayout.LayoutParams(-2, -2, Gravity.CENTER_VERTICAL or (if (choice) Gravity.START else Gravity.CENTER_HORIZONTAL)).apply {
                leftMargin = act.dp(20 + 16 * r.indent)
                rightMargin = act.dp(if (choice) 52 else 20)
                topMargin = act.dp(14)
                bottomMargin = act.dp(14)
            })
            if (r.checked) {
                row.addView(
                    ImageView(act).apply { setImageDrawable(Icon(Icon.Kind.CHECK, accent)) },
                    FrameLayout.LayoutParams(act.dp(22), act.dp(22), Gravity.END or Gravity.CENTER_VERTICAL).apply { rightMargin = act.dp(20) },
                )
                checkedRow = row
            }
            list.addView(row, LinearLayout.LayoutParams(-1, -2))
            if (i < rows.size - 1) {
                list.addView(View(act).apply { setBackgroundColor(sep) }, LinearLayout.LayoutParams(-1, 1).apply { leftMargin = act.dp(20) })
            }
        }
        val scroll = ScrollView(act).apply {
            addView(list)
            isVerticalScrollBarEnabled = false
            overScrollMode = View.OVER_SCROLL_NEVER
        }

        panel.addView(View(act).apply { background = rounded(sep, act.dpf(3)) }, LinearLayout.LayoutParams(act.dp(36), act.dp(5)).apply {
            gravity = Gravity.CENTER_HORIZONTAL
            topMargin = act.dp(8)
            bottomMargin = act.dp(4)
        })
        if (title != null) {
            panel.addView(label(act, title, 13f, muted, 600).apply {
                gravity = Gravity.CENTER
                maxLines = 2
                ellipsize = TextUtils.TruncateAt.END
            }, LinearLayout.LayoutParams(-1, -2).apply {
                leftMargin = act.dp(20); rightMargin = act.dp(20); topMargin = act.dp(8); bottomMargin = act.dp(10)
            })
        }
        panel.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        if (!choice) {
            panel.addView(View(act).apply { setBackgroundColor(act.res(R.color.group)) }, LinearLayout.LayoutParams(-1, act.dp(8)).apply { topMargin = act.dp(4) })
            panel.addView(label(act, "Отмена", 17f, accent, 600).apply {
                gravity = Gravity.CENTER
                isClickable = true
                setOnClickListener { close(null) }
                pressDim()
            }, LinearLayout.LayoutParams(-1, act.dp(56)))
        }

        val scrim = FrameLayout(act).apply {
            setBackgroundColor(Color.argb(110, 0, 0, 0))
            setOnClickListener { close(null) }
        }
        val lp = FrameLayout.LayoutParams(-1, if (choice) (screenH * 0.78f).toInt() else -2, Gravity.BOTTOM)
        scrim.addView(panel, lp)
        scrim.onInsets { _, bottom -> panel.setPadding(0, 0, 0, bottom) }
        d.setContentView(scrim)
        d.window?.apply {
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
            setDecorFitsSystemWindows(false)
            setDimAmount(0f)
        }
        // Список действий выше 70 % экрана прокручивается.
        if (!choice) panel.viewTreeObserver.addOnGlobalLayoutListener(object : ViewTreeObserver.OnGlobalLayoutListener {
            override fun onGlobalLayout() {
                panel.viewTreeObserver.removeOnGlobalLayoutListener(this)
                val max = (screenH * 0.7f).toInt()
                if (panel.height > max) {
                    lp.height = max
                    panel.layoutParams = lp
                }
            }
        })
        d.setOnShowListener {
            panel.translationY = screenH.toFloat()
            panel.animate().translationY(0f).setDuration(320).setInterpolator(DecelerateInterpolator(2.2f)).start()
            checkedRow?.let { r -> scroll.post { scroll.scrollTo(0, maxOf(r.top - scroll.height / 3, 0)) } }
        }
        d.show()
    }
}

/** Сегментный переключатель: выбранный сегмент — светлая «таблетка» на подложке. */
class Segmented(ctx: Context, labels: List<String>, selected: Int, private val onChange: (Int) -> Unit) : LinearLayout(ctx) {
    private val cells = ArrayList<TextView>()
    private var selected = selected

    init {
        orientation = HORIZONTAL
        val pad = ctx.dp(2)
        setPadding(pad, pad, pad, pad)
        background = rounded(ctx.res(R.color.group), ctx.dpf(9))
        labels.forEachIndexed { i, s ->
            val t = label(ctx, s, 14f, ctx.res(R.color.ink), 500).apply {
                gravity = Gravity.CENTER
                isClickable = true
                setOnClickListener {
                    if (this@Segmented.selected != i) {
                        this@Segmented.selected = i
                        paint()
                        tick()
                        onChange(i)
                    }
                }
            }
            cells += t
            addView(t, LayoutParams(0, ctx.dp(32), 1f))
        }
        paint()
    }

    private fun paint() {
        cells.forEachIndexed { i, t ->
            val on = i == selected
            t.background = if (on) rounded(context.res(R.color.cell), context.dpf(7)) else null
            t.elevation = if (on) context.dpf(1.5f) else 0f
            t.typeface = weight(if (on) 600 else 500)
        }
    }
}

/**
 * Обложка, нарисованная по названию: картинок в книгах пока нет (SPEC §2.9).
 * Цвет зависит только от названия, поэтому у книги он всегда один и тот же.
 */
class CoverView(ctx: Context, private val title: String, private val author: String = "", private val image: android.graphics.Bitmap? = null) : View(ctx) {
    private val tones = intArrayOf(
        0xFFB5593C.toInt(), 0xFF5F7A61.toInt(), 0xFF46607E.toInt(), 0xFF7A4E6D.toInt(),
        0xFFB08A3E.toInt(), 0xFF3F7373.toInt(), 0xFF8B4A4A.toInt(), 0xFF5B5B80.toInt(),
    )
    private val tone = tones[(title.hashCode() and 0x7fffffff) % tones.size]
    private val bg = Paint(Paint.ANTI_ALIAS_FLAG)
    private val rule = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(150, 255, 255, 255) }
    private val spine = Paint(Paint.ANTI_ALIAS_FLAG)
    private val ink = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(245, 255, 255, 255)
        typeface = Typeface.create(Typeface.SERIF, Typeface.BOLD)
    }
    private var layout: StaticLayout? = null
    private var authorLayout: StaticLayout? = null
    private val authorInk = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(215, 255, 255, 255)
        typeface = Typeface.create(Typeface.SERIF, Typeface.NORMAL)
    }

    init {
        clipToOutline = true
        outlineProvider = object : ViewOutlineProvider() {
            override fun getOutline(v: View, o: Outline) = o.setRoundRect(0, 0, v.width, v.height, ctx.dpf(5))
        }
        elevation = ctx.dpf(4)
        contentDescription = if (author.isBlank()) title else "$title, $author"
    }

    override fun onMeasure(w: Int, h: Int) {
        val width = MeasureSpec.getSize(w)
        setMeasuredDimension(width, (width * 1.5f).toInt())
    }

    override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) {
        bg.shader = LinearGradient(0f, 0f, w * 0.4f, h.toFloat(), scale(tone, 1.12f), scale(tone, 0.86f), Shader.TileMode.CLAMP)
        spine.shader = LinearGradient(0f, 0f, w * 0.09f, 0f, Color.argb(70, 0, 0, 0), Color.TRANSPARENT, Shader.TileMode.CLAMP)
        // Кегль — от ширины обложки; длинное название сжимается, пока не поместится.
        var size = w * 0.125f
        val inner = (w * 0.76f).toInt()
        while (true) {
            ink.textSize = size
            val l = StaticLayout.Builder.obtain(title, 0, title.length, ink, inner)
                .setAlignment(Layout.Alignment.ALIGN_CENTER).setMaxLines(6).setEllipsize(TextUtils.TruncateAt.END)
                .setLineSpacing(0f, 1.05f).setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NORMAL).build()
            layout = l
            if (l.height < h * 0.5f || size < w * 0.07f) break
            size *= 0.92f
        }
        authorLayout = author.takeIf { it.isNotBlank() }?.let {
            authorInk.textSize = w * 0.085f
            StaticLayout.Builder.obtain(it, 0, it.length, authorInk, inner)
                .setAlignment(Layout.Alignment.ALIGN_CENTER).setMaxLines(2).setEllipsize(TextUtils.TruncateAt.END).build()
        }
    }

    override fun onDraw(c: Canvas) {
        if (image != null) {
            // Обложка из сети: по центру, с обрезкой лишнего; тень корешка — как у нарисованной.
            val k = maxOf(width / image.width.toFloat(), height / image.height.toFloat())
            val dw = image.width * k
            val dh = image.height * k
            c.drawBitmap(image, null, android.graphics.RectF((width - dw) / 2, (height - dh) / 2, (width + dw) / 2, (height + dh) / 2), Paint(Paint.FILTER_BITMAP_FLAG))
            c.drawRect(0f, 0f, width * 0.09f, height.toFloat(), spine)
            return
        }
        c.drawRect(0f, 0f, width.toFloat(), height.toFloat(), bg)
        c.drawRect(0f, 0f, width * 0.09f, height.toFloat(), spine)
        val l = layout ?: return
        c.save()
        c.translate((width - l.width) / 2f + width * 0.02f, height * 0.2f)
        l.draw(c)
        c.restore()
        val y = height * 0.2f + l.height + height * 0.05f
        c.drawRect(width / 2f - width * 0.09f, y, width / 2f + width * 0.09f, y + maxOf(2f, width * 0.012f), rule)
        authorLayout?.let {
            c.save()
            c.translate((width - it.width) / 2f + width * 0.02f, y + height * 0.04f)
            it.draw(c)
            c.restore()
        }
    }

    private fun scale(c: Int, k: Float) =
        Color.rgb(min(255, (Color.red(c) * k).toInt()), min(255, (Color.green(c) * k).toInt()), min(255, (Color.blue(c) * k).toInt()))
}

/**
 * Экран с темой из настроек приложения (а не только системной). Если тему сменили
 * на другом экране, при возвращении экран пересоздаётся.
 */
abstract class ThemedActivity : Activity() {
    private var themeAtCreate = ""

    override fun attachBaseContext(base: Context) {
        val s = Settings(base)
        themeAtCreate = s.theme
        s.themeConfig(base)?.let { applyOverrideConfiguration(it) }
        super.attachBaseContext(base)
    }

    override fun onResume() {
        super.onResume()
        if (Settings(this).theme != themeAtCreate) recreate()
    }
}
