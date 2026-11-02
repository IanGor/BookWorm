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
    @Test fun selectionSurvivesViewportChangesAcrossParagraphsAndChapters() {
        val text = "Первая глава\n\nАбзац один.\n\nАбзац два.\n\nВторая глава\n\nНачало 😀 и конец."
        val all = selection(text)
        val start = text.indexOf("один")
        val end = text.indexOf(" и конец")
        val first = all.cells.first { it.start == start }
        all.updateCells(all.cells.filter { it.end <= text.indexOf("Абзац два") })
        all.wordAt(first.left, first.top)
        val fixedStart = all.start
        val secondViewport = selection(text).cells.filter { it.start >= text.indexOf("Вторая глава") }
        all.updateCells(secondViewport)
        assertEquals(fixedStart, all.start)
        val last = secondViewport.first { it.end == end }
        all.moveHandle(false, last.right, last.top)
        assertEquals("один.\n\nАбзац два.\n\nВторая глава\n\nНачало 😀", all.selected)
        all.updateCells(emptyList())
        assertEquals(fixedStart, all.start)
        assertEquals(end, all.end)
        all.selectAll()
        assertEquals(text, all.selected)
    }

    @Test fun startHandleCanExtendBackAcrossChapters() {
        val text = "Первая глава\n\nНачало.\n\nВторая глава\n\nКонец."
        val all = selection(text)
        val cells = all.cells
        val last = cells.first { it.start == text.indexOf("Конец") }
        all.updateCells(cells.filter { it.start >= text.indexOf("Конец") })
        all.wordAt(last.left, last.top)
        val end = all.end
        all.updateCells(cells.filter { it.end <= text.indexOf("Вторая глава") })
        val first = all.cells.first { it.start == text.indexOf("Начало") }
        all.moveHandle(true, first.left, first.top)
        assertEquals(text.substring(text.indexOf("Начало"), end), all.selected)
    }

    @Test fun oneViewportCanContainTwoChapters() {
        val cells = TextSelection.parseCells("0 1 10 970 30 1010", 950f) +
            TextSelection.parseCells("3 4 10 20 30 60", -100f)
        val s = TextSelection("а\n\nб", cells)
        s.wordAt(20f, 40f)
        assertEquals("а", s.selected)
        s.moveHandle(false, 20f, 140f)
        assertEquals("а\n\nб", s.selected)
    }

}
