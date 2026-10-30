//! Общая модель документа: в неё превращаются и FB2, и EPUB.
//!
//! Книга — плоская последовательность блоков. Вложенность (эпиграф, цитата,
//! стихотворение) хранится в `Context` каждого блока. Позиция чтения —
//! (номер блока, номер символа в нём); она не зависит от шрифта, кегля и экрана.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InlineStyle {
    pub italic: bool,
    pub bold: bool,
    pub sup: bool,
    pub sub: bool,
    pub strike: bool,
    pub code: bool,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub text: String,
    pub style: InlineStyle,
    /// id сноски, если этот фрагмент — ссылка на неё.
    pub note: Option<String>,
}

impl Span {
    pub fn plain(text: &str) -> Span {
        Span { text: text.to_string(), style: InlineStyle::default(), note: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Ru,
    En,
}

impl Lang {
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("en") {
            Lang::En
        } else {
            Lang::Ru
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Заголовок: 0 — заголовок книги, 1 — глава, дальше вложенные разделы.
    /// Заголовок из нескольких `<p>` — несколько блоков подряд.
    Heading(u8),
    Subtitle,
    Para,
    TextAuthor,
    PoemTitle,
    Verse,
    Date,
    TableRow,
    EmptyLine,
    StanzaGap,
    Image(String),
    PageBreak,
    /// Преформатированный текст (`<pre>`): строка как есть, моноширинным, без выключки.
    Pre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    pub epigraph: bool,
    pub cite: bool,
    pub poem: bool,
}

/// Что взяли из CSS книги для абзаца (EPUB). У FB2 — всё по умолчанию.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockCss {
    /// Только центр и право: левое выравнивание издателя не берём — выключка всегда.
    pub align: Option<crate::para::Align>,
    /// Отступ первой строки в em; отрицательный — висячая первая строка.
    pub indent: Option<f32>,
    /// Поля абзаца: em и доли ширины текста.
    pub left_em: f32,
    pub left_pct: f32,
    pub right_em: f32,
    pub right_pct: f32,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: Kind,
    pub ctx: Context,
    /// Текст уже прошёл типограф.
    pub spans: Vec<Span>,
    pub lang: Lang,
    pub css: BlockCss,
}

impl Block {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub title: String,
    pub level: u8,
    pub block: usize,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub mime: String,
    pub base64: String,
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub title: String,
    pub authors: Vec<String>,
    pub lang: Lang,
    pub blocks: Vec<Block>,
    /// Сноски и комментарии: id → блоки текста сноски.
    pub notes: HashMap<String, Vec<Block>>,
    pub toc: Vec<TocEntry>,
    pub cover: Option<String>,
    pub images: HashMap<String, Image>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Position {
    pub block: u32,
    pub offset: u32,
}
