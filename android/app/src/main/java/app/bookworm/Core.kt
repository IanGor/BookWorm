package app.bookworm

import java.nio.ByteBuffer

/**
 * Ядро на Rust (crates/android). Вёрстка и отрисовка страниц — там; здесь только
 * объявления. Номер книги (handle) — указатель на книгу в Rust, 0 — книги нет.
 * Вызывать из одного фонового потока: функции могут работать долго (вёрстка).
 */
object Core {
    init {
        System.loadLibrary("bookworm")
    }

    external fun lastError(): String
    /** `fonts` — четыре начертания; `step` — размер в px; `scroll` — прокрутка; `bionic` — выделение начал слов. */
    external fun open(
        path: String, fonts: String, mono: String,
        width: Int, height: Int, step: Float, scroll: Boolean, bionic: Boolean,
    ): Long

    /** Гарнитуры из файлов: строки «название, кириллица 0/1, четыре начертания» через табуляцию. */
    external fun fontFamilies(paths: String): String?
    /** Ступени размера через пробел и номер ступени по умолчанию последним. */
    external fun sizeSteps(): String

    // Прокрутка: страница = глава.
    external fun pageHeight(h: Long, page: Int): Float
    external fun renderStrip(h: Long, page: Int, top: Float, height: Int, night: Boolean, out: ByteBuffer): Boolean
    external fun footerHeight(h: Long): Int
    external fun renderFooter(h: Long, page: Int, top: Float, night: Boolean, out: ByteBuffer): Boolean
    external fun posAt(h: Long, page: Int, top: Float): Long
    external fun topOf(h: Long, page: Int, pos: Long): Float
    external fun close(h: Long)
    external fun title(h: Long): String?
    external fun blockAt(h: Long, page: Int, x: Float, y: Float): Int
    external fun blockText(h: Long, block: Int): String?
    external fun typographyReport(h: Long, block: Int): String?
    /** Название и авторы из файла без вёрстки: «название\nавтор1; автор2». */
    external fun meta(path: String): String?
    external fun pageCount(h: Long): Int
    external fun render(h: Long, page: Int, night: Boolean, out: ByteBuffer): Boolean
    external fun pageStart(h: Long, page: Int): Long
    external fun pageOf(h: Long, pos: Long): Int
    external fun hitNote(h: Long, page: Int, x: Float, y: Float): String?
    external fun noteAreas(h: Long, page: Int): String?
    external fun toc(h: Long): String?
    external fun noteHeight(h: Long, id: String): Int
    external fun renderNote(h: Long, id: String, night: Boolean, out: ByteBuffer): Boolean
}
