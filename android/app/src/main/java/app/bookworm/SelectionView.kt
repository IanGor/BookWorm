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
    private val report: (String) -> Unit,
    private val closed: () -> Unit,
) : View(ctx) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val radius = 10f * resources.displayMetrics.density
    private var mode: ActionMode? = null
    private var dragging: Boolean? = null

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
                    3 -> { val text = selection.selected; mode.finish(); report(text) }
                    else -> return false
                }
                return true
            }
            override fun onDestroyActionMode(mode: ActionMode) { this@SelectionView.mode = null; closed() }
            override fun onGetContentRect(mode: ActionMode, view: View, outRect: Rect) {
                val cell = selection.cells.firstOrNull { it.end > selection.start } ?: return
                outRect.set(cell.left.toInt(), cell.top.toInt().coerceAtLeast(0), cell.right.toInt(), cell.bottom.toInt().coerceAtLeast(1))
            }
        }, ActionMode.TYPE_FLOATING)
        if (mode == null) closed()
        invalidate()
    }

    fun finish() { mode?.finish() }

    private fun firstCell() = selection.cells.firstOrNull { it.end > selection.start && it.start < selection.end }
    private fun lastCell() = selection.cells.lastOrNull { it.end > selection.start && it.start < selection.end }

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
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val first = firstCell()
                val last = lastCell()
                fun distance(x: Float, y: Float) = kotlin.math.hypot(event.x - x, event.y - y)
                val a = first?.let { distance(it.left, it.bottom + radius) } ?: Float.MAX_VALUE
                val b = last?.let { distance(it.right, it.bottom + radius) } ?: Float.MAX_VALUE
                dragging = if (minOf(a, b) <= radius * 3) a <= b else null
                if (dragging == null) { finish(); return true }
                mode?.hide(60_000)
            }
            MotionEvent.ACTION_MOVE -> dragging?.let {
                selection.moveHandle(it, event.x, event.y - radius)
                invalidate()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                dragging = null
                mode?.hide(0)
                mode?.invalidateContentRect()
            }
        }
        return true
    }
}
