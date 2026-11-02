package app.bookworm

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.View

/** Selection over the engine's bitmap, without reflowing the book in a TextView. */
internal class SelectionView(
    ctx: Context,
    private val selection: TextSelection,
    private val continuous: Boolean,
    private val advance: (Float, (List<SelectionCell>?) -> Unit) -> Unit,
    private val report: (Int, Int, String) -> Unit,
    private val closed: () -> Unit,
) : View(ctx) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val radius = 10f * resources.displayMetrics.density
    private var mode: ActionMode? = null
    private var dragging: Boolean? = null
    private var pointerX = 0f
    private var pointerY = 0f
    private var downX = 0f
    private var downY = 0f
    private var panning = false
    private var loading = false
    private var nextAdvanceAt = 0L
    private val edgeSize = 48f * resources.displayMetrics.density
    private val autoScroll = object : Runnable {
        override fun run() {
            if (dragging == null || loading) return
            val now = android.os.SystemClock.uptimeMillis()
            if (now < nextAdvanceAt) { postDelayed(this, nextAdvanceAt - now); return }
            val direction = when {
                pointerY < edgeSize -> -1
                pointerY > height - edgeSize -> 1
                else -> return
            }
            nextAdvanceAt = now + if (continuous) 50L else 650L
            moveViewport(direction * height * 0.07f)
        }
    }

    private fun moveViewport(dy: Float) {
        if (loading) return
        loading = true
        advance(dy) { cells ->
            loading = false
            if (mode == null || cells == null) return@advance
            selection.updateCells(cells)
            dragging?.let { selection.moveHandle(it, pointerX, pointerY - radius) }
            invalidate()
            mode?.invalidateContentRect()
            if (dragging != null) postDelayed(autoScroll, if (continuous) 50L else 650L)
        }
    }

    init { isFocusableInTouchMode = true }

    fun begin(x: Float, y: Float) {
        selection.wordAt(x, y)
        requestFocus()
        mode = startActionMode(object : ActionMode.Callback2() {
            override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
                menu.add(0, 1, 0, "Копировать").setShowAsAction(MenuItem.SHOW_AS_ACTION_IF_ROOM)
                menu.add(0, 2, 1, "Выделить всё")
                menu.add(0, 3, 2, "Ошибка").setShowAsAction(MenuItem.SHOW_AS_ACTION_IF_ROOM)
                return true
            }
            override fun onPrepareActionMode(mode: ActionMode, menu: Menu) = false
            override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
                when (item.itemId) {
                    1 -> {
                        context.getSystemService(ClipboardManager::class.java)
                            .setPrimaryClip(ClipData.newPlainText("Текст книги", selection.selected))
                        mode.finish()
                    }
                    2 -> { selection.selectAll(); invalidate(); mode.invalidateContentRect() }
                    3 -> {
                        val start = selection.start
                        val end = selection.end
                        val text = selection.selected
                        mode.finish()
                        report(start, end, text)
                    }
                    else -> return false
                }
                return true
            }
            override fun onDestroyActionMode(mode: ActionMode) {
                removeCallbacks(autoScroll)
                this@SelectionView.mode = null
                closed()
            }
            override fun onGetContentRect(mode: ActionMode, view: View, outRect: Rect) {
                val cell = selection.cells.firstOrNull { it.end > selection.start && it.start < selection.end } ?: selection.cells.firstOrNull() ?: return
                outRect.set(cell.left.toInt(), cell.top.toInt().coerceAtLeast(0), cell.right.toInt(), cell.bottom.toInt().coerceAtLeast(1))
            }
        }, ActionMode.TYPE_FLOATING)
        if (mode == null) closed()
        invalidate()
    }

    fun finish() { mode?.finish() }

    fun navigate(dy: Float) { moveViewport(dy) }

    private fun firstCell() = selection.cells.firstOrNull { it.start <= selection.start && it.end > selection.start }
    private fun lastCell() = selection.cells.lastOrNull { it.start < selection.end && it.end >= selection.end }

    override fun onDraw(canvas: Canvas) {
        paint.color = 0x663399ff
        for (cell in selection.cells) {
            if (cell.end > selection.start && cell.start < selection.end) {
                canvas.drawRect(cell.left, cell.top, cell.right, cell.bottom, paint)
            }
        }
        paint.color = 0xff3377cc.toInt()
        firstCell()?.let { canvas.drawCircle(it.left, it.bottom + radius, radius, paint) }
        lastCell()?.let { canvas.drawCircle(it.right, it.bottom + radius, radius, paint) }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val previousY = pointerY
        pointerX = event.x
        pointerY = event.y
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val first = firstCell()
                val last = lastCell()
                fun distance(x: Float, y: Float) = kotlin.math.hypot(event.x - x, event.y - y)
                val a = first?.let { distance(it.left, it.bottom + radius) } ?: Float.MAX_VALUE
                val b = last?.let { distance(it.right, it.bottom + radius) } ?: Float.MAX_VALUE
                dragging = if (minOf(a, b) <= radius * 3) a <= b else null
                downX = event.x
                downY = event.y
                nextAdvanceAt = 0L
                panning = false
                mode?.hide(60_000)
            }
            MotionEvent.ACTION_MOVE -> {
                val handle = dragging
                if (handle != null) {
                    selection.moveHandle(handle, pointerX, pointerY - radius)
                    invalidate()
                    removeCallbacks(autoScroll)
                    if (!loading) post(autoScroll)
                } else if (panning || kotlin.math.hypot(pointerX - downX, pointerY - downY) > radius) {
                    panning = true
                    if (continuous) moveViewport(previousY - pointerY)
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                removeCallbacks(autoScroll)
                if (event.actionMasked == MotionEvent.ACTION_UP && dragging == null) {
                    if (!panning) finish()
                    else if (!continuous) {
                        val dx = downX - pointerX
                        val dy = downY - pointerY
                        moveViewport(if (kotlin.math.abs(dx) > kotlin.math.abs(dy)) dx else dy)
                    }
                }
                dragging = null
                mode?.hide(0)
                mode?.invalidateContentRect()
            }
        }
        return true
    }
}
