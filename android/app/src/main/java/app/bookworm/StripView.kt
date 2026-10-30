package app.bookworm

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.widget.OverScroller

/**
 * Непрерывная прокрутка: главы идут лентой одна за другой. Ядро верстает главу
 * одной длинной полосой и рисует её кусками (плитками) высотой в полэкрана;
 * этот вид только раскладывает готовые плитки и ведёт прокрутку с инерцией.
 * Строка статуса — отдельная полоса поверх нижнего поля.
 */
class StripView(ctx: Context, private val host: Host) : View(ctx) {
    interface Host {
        /** Готовая плитка или null (тогда хозяин её закажет и позже вызовет invalidate). */
        fun tile(chapter: Int, index: Int): Bitmap?
        fun footer(): Bitmap?
        fun onTap(x: Float, y: Float)
        /** Прокрутка остановилась: сохранить позицию, обновить строку статуса. */
        fun onSettled(chapter: Int, top: Float)
        fun onResize(width: Int, height: Int)
    }

    /** Высоты глав в px; задаёт хозяин после вёрстки. */
    var heights = FloatArray(0)
        set(v) {
            field = v
            normalize()
            invalidate()
        }

    /** Текущая глава и верх окна в её координатах. */
    var chapter = 0
        private set
    var top = 0f
        private set

    val tileHeight: Int get() = maxOf(height / 2, 1)
    var background = 0
    private val scroller = OverScroller(ctx)
    private var lastY = 0
    private val settle = Runnable { host.onSettled(chapter, top) }

    private val gestures = GestureDetector(ctx, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean {
            scroller.forceFinished(true)
            return true
        }

        override fun onScroll(e1: MotionEvent?, e2: MotionEvent, dx: Float, dy: Float): Boolean {
            scrollBy(dy)
            return true
        }

        override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
            lastY = 0
            scroller.fling(0, 0, 0, -vy.toInt(), 0, 0, Int.MIN_VALUE / 2, Int.MAX_VALUE / 2)
            postInvalidateOnAnimation()
            return true
        }

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            host.onTap(e.x, e.y)
            return true
        }
    }).apply { setIsLongpressEnabled(false) }

    fun jumpTo(chapter: Int, top: Float) {
        scroller.forceFinished(true)
        this.chapter = chapter.coerceIn(0, maxOf(heights.size - 1, 0))
        this.top = top
        normalize()
        invalidate()
        moved()
    }

    /** Сдвиг на `dy` px вниз по тексту (отрицательный — вверх). */
    fun scrollBy(dy: Float) {
        if (heights.isEmpty()) return
        top += dy
        normalize()
        invalidate()
        moved()
    }

    private fun moved() {
        removeCallbacks(settle)
        postDelayed(settle, 250)
    }

    /** Переход между главами и упор в начало и конец книги. */
    private fun normalize() {
        if (heights.isEmpty()) return
        chapter = chapter.coerceIn(0, heights.size - 1)
        while (top >= heights[chapter] && chapter < heights.size - 1) {
            top -= heights[chapter]
            chapter++
        }
        while (top < 0 && chapter > 0) {
            chapter--
            top += heights[chapter]
        }
        if (top < 0) top = 0f
        // Последняя глава: не уезжаем за её конец.
        if (chapter == heights.size - 1) top = top.coerceAtMost(maxOf(heights[chapter] - height, 0f))
    }

    override fun computeScroll() {
        if (scroller.computeScrollOffset()) {
            val y = scroller.currY
            scrollBy((y - lastY).toFloat())
            lastY = y
            postInvalidateOnAnimation()
        }
    }

    private val fill = Paint()

    override fun onDraw(c: Canvas) {
        c.drawColor(background)
        if (heights.isEmpty()) return
        val th = tileHeight
        var ch = chapter
        var t = top
        var screenY = 0f
        // Плитки сверху вниз, с переходом в следующую главу.
        while (screenY < height && ch < heights.size) {
            val index = (t / th).toInt()
            val tileTop = index * th.toFloat()
            val drawY = screenY - (t - tileTop)
            val chapterEnd = heights[ch] - tileTop
            host.tile(ch, index)?.let { c.drawBitmap(it, 0f, drawY, null) }
            if (chapterEnd <= th) {
                // Конец главы внутри этой плитки: следующая глава сразу за ним.
                screenY = drawY + chapterEnd
                ch++
                t = 0f
                // Хвост плитки за концом главы закрывается следующей главой или фоном.
                if (ch < heights.size) c.drawRect(0f, screenY, width.toFloat(), drawY + th, fill.apply { color = background })
            } else {
                screenY = drawY + th
                t = tileTop + th
            }
        }
        host.footer()?.let { c.drawBitmap(it, 0f, (height - it.height).toFloat(), null) }
    }

    override fun onTouchEvent(e: MotionEvent): Boolean = gestures.onTouchEvent(e) || super.onTouchEvent(e)

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        if (w > 0 && h > 0) host.onResize(w, h)
    }
}
