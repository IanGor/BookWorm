package app.bookworm

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import kotlin.math.abs

/** Показывает готовую страницу из ядра пиксель в пиксель и сообщает о жестах. */
class PageView(ctx: Context, private val listener: Listener) : View(ctx) {
    interface Listener {
        fun onTap(x: Float, y: Float)
        fun onSwipe(forward: Boolean)
        fun onResize(width: Int, height: Int)
    }

    private var bitmap: Bitmap? = null
    private var message: String? = null
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textSize = 46f
        typeface = Typeface.SERIF
        textAlign = Paint.Align.CENTER
    }

    private val gestures = GestureDetector(ctx, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent) = true

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            listener.onTap(e.x, e.y)
            return true
        }

        override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
            if (abs(vx) < abs(vy) || abs(vx) < 300) return false
            listener.onSwipe(vx < 0)
            return true
        }
    }).apply { setIsLongpressEnabled(false) }

    fun show(b: Bitmap) {
        bitmap = b
        message = null
        invalidate()
    }

    fun message(m: String, color: Int) {
        bitmap = null
        message = m
        text.color = color
        invalidate()
    }

    override fun onDraw(c: Canvas) {
        bitmap?.let {
            c.drawBitmap(it, 0f, 0f, null)
            return
        }
        val lines = message?.split('\n') ?: return
        var y = height / 2f - (lines.size - 1) * text.textSize * 0.7f
        for (l in lines) {
            c.drawText(l, width / 2f, y, text)
            y += text.textSize * 1.4f
        }
    }

    override fun onTouchEvent(e: MotionEvent): Boolean = gestures.onTouchEvent(e) || super.onTouchEvent(e)

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        if (w > 0 && h > 0) listener.onResize(w, h)
    }
}
