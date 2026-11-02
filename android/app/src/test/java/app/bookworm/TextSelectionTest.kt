package app.bookworm

import org.junit.Assert.*
import org.junit.Test

class TextSelectionTest {
    private val sample = javaClass.getResource("/selection-ru.txt")!!.readText().trim()

    // Split words across lines and vary glyph widths, as with bionic runs and justification.
    private fun selection(text: String = sample): TextSelection {
        var offset = 0
        val rows = text.codePoints().toArray().mapIndexed { i, cp ->
            val start = offset
            offset += Character.charCount(cp)
            val x = 88 + (i % 34) * 20
            val y = 9410 + (i / 34) * 64
            "$start $offset $x $y ${x + 18} ${y + 48}"
        }.joinToString("\n")
        return TextSelection.parse(text, rows, 9300f)
    }

    @Test fun longPressSelectsWordInsteadOfParagraph() {
        val s = selection()
        val cell = s.cells.first { it.start == sample.indexOf("классный") + 3 }
        s.wordAt(cell.left + 3, cell.top + 3)
        assertEquals("классный", s.selected)
        assertEquals(110f, s.cells.first().top)
    }

    @Test fun nbspDoesNotJoinWordsAndLineBreakDoesNotSplitWord() {
        val s = selection()
        for (word in listOf("своих", "рассказывали", "похожее", "что-то", "возникло")) {
            val cell = s.cells.first { it.start == sample.indexOf(word) + 2 }
            s.wordAt(cell.left, cell.top)
            assertEquals(word, s.selected)
        }
    }

    @Test fun handlesSelectExactReportFragmentAndCannotCross() {
        val s = selection()
        val first = s.cells.first { it.start == sample.indexOf("Про пластмассу") }
        val last = s.cells.first { it.end == sample.indexOf("установки.") + "установки.".length }
        s.wordAt(first.left, first.top)
        s.moveHandle(false, last.right, last.top)
        assertEquals("Про пластмассу, цеха, установки.", s.selected)
        s.moveHandle(true, last.right, last.top)
        assertEquals(".", s.selected)
        s.moveHandle(false, first.left, first.top)
        assertEquals(".", s.selected)
        s.selectAll()
        assertEquals(sample, s.selected)
    }

    @Test fun handlesKeepSupplementaryCharactersIntact() {
        val s = selection("а 😀 б")
        val emoji = s.cells[2]
        s.wordAt(emoji.left, emoji.top)
        assertEquals("😀", s.selected)
        val after = s.cells.last()
        s.moveHandle(true, after.left, after.top)
        assertEquals("😀", s.selected)
    }
}
