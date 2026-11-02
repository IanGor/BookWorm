package app.bookworm

/** Offsets are UTF-16, just like String.substring and Android's selection APIs. */
internal data class SelectionCell(val start: Int, val end: Int, val left: Float, val top: Float, val right: Float, val bottom: Float)

internal class TextSelection(val text: String, val cells: List<SelectionCell>) {
    var start = 0
        private set
    var end = 0
        private set
    val selected: String get() = text.substring(start, end)

    fun nearest(x: Float, y: Float): SelectionCell? = cells.minByOrNull {
        val dy = maxOf(it.top - y, y - it.bottom, 0f)
        val dx = maxOf(it.left - x, x - it.right, 0f)
        // Prefer the touched line over an adjacent line's closer horizontal position.
        dy * 10000f + dx
    }

    fun wordAt(x: Float, y: Float) {
        val cell = nearest(x, y) ?: return
        start = cell.start
        end = cell.end
        fun word(c: Char) = c.isLetterOrDigit() || c == '-' || c == '\u00ad' || Character.getType(c) == Character.NON_SPACING_MARK.toInt()
        if (word(text[start])) {
            while (start > 0 && word(text[start - 1])) start--
            while (end < text.length && word(text[end])) end++
        }
    }

    fun moveHandle(first: Boolean, x: Float, y: Float) {
        val cell = nearest(x, y) ?: return
        if (first) { if (cell.start < end) start = cell.start }
        else { if (cell.end > start) end = cell.end }
    }

    fun selectAll() { start = 0; end = text.length }

    companion object {
        fun parse(text: String, geometry: String, shiftY: Float): TextSelection {
            val cells = geometry.lineSequence().filter { it.isNotBlank() }.map { row ->
                val v = row.split(' ')
                SelectionCell(v[0].toInt(), v[1].toInt(), v[2].toFloat(), v[3].toFloat() - shiftY,
                    v[4].toFloat(), v[5].toFloat() - shiftY)
            }.toList()
            return TextSelection(text, cells)
        }
    }
}
