//! Разбор FictionBook 2 в общую модель документа.

use crate::model::{Block, Context, Document, Image, InlineStyle, Kind, Lang, Span, TocEntry};
use crate::typo;
use crate::xml::{self, Element, Node};
use std::collections::HashSet;

pub fn parse(bytes: &[u8]) -> Result<Document, String> {
    let text = decode(bytes);
    let root = xml::parse(&text);
    let fb = root.find("fictionbook").ok_or("нет элемента FictionBook")?;
    let info = fb.find("title-info");
    let lang = info.and_then(|i| i.child("lang")).map_or(Lang::Ru, |l| Lang::from_code(&l.text()));

    let mut doc = Document { lang, ..Default::default() };
    if let Some(info) = info {
        doc.title = collapse(&info.child("book-title").map(|t| t.text()).unwrap_or_default());
        for a in info.elements().filter(|e| e.name == "author") {
            let name: Vec<String> = ["first-name", "middle-name", "last-name", "nickname"]
                .iter()
                .filter_map(|n| a.child(n).map(|e| collapse(&e.text())))
                .filter(|s| !s.is_empty())
                .collect();
            if !name.is_empty() {
                doc.authors.push(name.join(" "));
            }
        }
        doc.cover = info.child("coverpage").and_then(|c| c.child("image")).and_then(href);
    }

    // Сноски собираем заранее: ссылку без type="note" узнаём по id цели.
    let mut note_ids = HashSet::new();
    for body in fb.elements().filter(|e| e.name == "body" && is_notes(e)) {
        collect_ids(body, &mut note_ids);
    }

    let mut main = Conv::new(lang, &note_ids);
    for body in fb.elements().filter(|e| e.name == "body") {
        if is_notes(body) {
            for sec in sections_with_id(body) {
                let mut c = Conv::new(lang, &note_ids);
                c.note(sec);
                if let Some(id) = sec.attr("id") {
                    doc.notes.entry(id.to_string()).or_insert(c.blocks);
                }
            }
        } else {
            main.body(body);
        }
    }
    doc.blocks = main.blocks;
    doc.toc = main.toc;

    for bin in fb.elements().filter(|e| e.name == "binary") {
        if let Some(id) = bin.attr("id") {
            let mime = bin.attr("content-type").unwrap_or_default().to_string();
            doc.images.insert(id.to_string(), Image { mime, base64: bin.text() });
        }
    }
    Ok(doc)
}

/// Кодировка берётся из XML-декларации; по умолчанию UTF-8.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]).to_ascii_lowercase();
    let label = head.find("encoding=").and_then(|p| {
        let rest = &head[p + 9..];
        let q = rest.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        rest[1..].find(q).map(|e| rest[1..1 + e].to_string())
    });
    let enc = label.and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes())).unwrap_or(encoding_rs::UTF_8);
    let (s, _, _) = enc.decode(bytes);
    s.into_owned()
}

fn is_notes(body: &Element) -> bool {
    matches!(body.attr("name"), Some("notes" | "comments"))
}

fn collect_ids(e: &Element, out: &mut HashSet<String>) {
    for c in e.elements() {
        if let Some(id) = c.attr("id") {
            out.insert(id.to_string());
        }
        collect_ids(c, out);
    }
}

fn sections_with_id(body: &Element) -> Vec<&Element> {
    let mut out = Vec::new();
    fn walk<'a>(e: &'a Element, out: &mut Vec<&'a Element>) {
        for c in e.elements().filter(|c| c.name == "section") {
            if c.attr("id").is_some() {
                out.push(c);
            } else {
                walk(c, out);
            }
        }
    }
    walk(body, &mut out);
    out
}

fn href(e: &Element) -> Option<String> {
    e.attr("href").map(|h| h.trim_start_matches('#').to_string()).filter(|h| !h.is_empty())
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Conv<'a> {
    lang: Lang,
    notes: &'a HashSet<String>,
    blocks: Vec<Block>,
    toc: Vec<TocEntry>,
}

impl<'a> Conv<'a> {
    fn new(lang: Lang, notes: &'a HashSet<String>) -> Self {
        Conv { lang, notes, blocks: Vec::new(), toc: Vec::new() }
    }

    fn push(&mut self, kind: Kind, ctx: Context, mut spans: Vec<Span>) {
        typo::apply(&mut spans, self.lang);
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }
        self.blocks.push(Block { kind, ctx, spans, lang: self.lang, css: Default::default() });
    }

    fn mark(&mut self, kind: Kind, ctx: Context) {
        self.blocks.push(Block { kind, ctx, spans: Vec::new(), lang: self.lang, css: Default::default() });
    }

    fn page_break(&mut self) {
        if !matches!(self.blocks.last().map(|b| &b.kind), None | Some(Kind::PageBreak)) {
            self.mark(Kind::PageBreak, Context::default());
        }
    }

    fn body(&mut self, body: &Element) {
        for e in body.elements() {
            match e.name.as_str() {
                "title" => {
                    self.title(e, 0);
                    self.page_break();
                }
                "section" => self.section(e, 1),
                _ => self.flow_item(e, Context::default()),
            }
        }
    }

    fn section(&mut self, s: &Element, depth: u8) {
        if depth == 1 {
            self.page_break();
        }
        for e in s.elements() {
            match e.name.as_str() {
                "title" => self.title(e, depth),
                "section" => self.section(e, depth.saturating_add(1)),
                _ => self.flow_item(e, Context::default()),
            }
        }
    }

    fn title(&mut self, t: &Element, level: u8) {
        let start = self.blocks.len();
        for p in t.elements().filter(|e| e.name == "p") {
            let spans = self.inline(p);
            self.push(Kind::Heading(level), Context::default(), spans);
        }
        if level > 0 && self.blocks.len() > start {
            let title = self.blocks[start..].iter().map(|b| b.text()).collect::<Vec<_>>().join(" ");
            self.toc.push(TocEntry { title, level, block: start });
        }
    }

    /// Сноска: всё, кроме заголовка с её номером.
    fn note(&mut self, sec: &Element) {
        for e in sec.elements().filter(|e| e.name != "title") {
            self.flow_item(e, Context::default());
        }
    }

    fn flow(&mut self, e: &Element, ctx: Context) {
        for c in e.elements() {
            self.flow_item(c, ctx);
        }
    }

    fn flow_item(&mut self, e: &Element, ctx: Context) {
        match e.name.as_str() {
            "p" => {
                let spans = self.inline(e);
                self.push(Kind::Para, ctx, spans);
            }
            "subtitle" => {
                let spans = self.inline(e);
                self.push(Kind::Subtitle, ctx, spans);
            }
            "text-author" => {
                let spans = self.inline(e);
                self.push(Kind::TextAuthor, ctx, spans);
            }
            "empty-line" => self.mark(Kind::EmptyLine, ctx),
            "epigraph" => self.flow(e, Context { epigraph: true, ..ctx }),
            "cite" | "annotation" => self.flow(e, Context { cite: true, ..ctx }),
            "poem" => self.poem(e, ctx),
            "table" => self.table(e, ctx),
            "image" => {
                if let Some(id) = href(e) {
                    self.mark(Kind::Image(id), ctx);
                }
            }
            "section" => self.section(e, 2),
            "title" => {
                for p in e.elements().filter(|p| p.name == "p") {
                    let spans = self.inline(p);
                    self.push(Kind::Subtitle, ctx, spans);
                }
            }
            // Неизвестный тег с текстом — показываем как абзац, контейнер — обходим.
            _ if e.elements().next().is_some() => {
                if e.children.iter().any(|n| matches!(n, Node::Text(t) if !t.trim().is_empty())) {
                    let spans = self.inline(e);
                    self.push(Kind::Para, ctx, spans);
                } else {
                    self.flow(e, ctx);
                }
            }
            _ => {
                if !e.text().trim().is_empty() {
                    let spans = self.inline(e);
                    self.push(Kind::Para, ctx, spans);
                }
            }
        }
    }

    fn poem(&mut self, poem: &Element, ctx: Context) {
        let ctx = Context { poem: true, ..ctx };
        let mut stanzas = 0;
        for e in poem.elements() {
            match e.name.as_str() {
                "title" => {
                    for p in e.elements().filter(|p| p.name == "p") {
                        let spans = self.inline(p);
                        self.push(Kind::PoemTitle, ctx, spans);
                    }
                }
                "epigraph" => self.flow(e, Context { epigraph: true, ..ctx }),
                "stanza" => {
                    if stanzas > 0 {
                        self.mark(Kind::StanzaGap, ctx);
                    }
                    stanzas += 1;
                    for v in e.elements() {
                        let kind = match v.name.as_str() {
                            "v" => Kind::Verse,
                            "title" | "subtitle" => Kind::PoemTitle,
                            _ => continue,
                        };
                        if v.name == "title" {
                            for p in v.elements().filter(|p| p.name == "p") {
                                let spans = self.inline(p);
                                self.push(Kind::PoemTitle, ctx, spans);
                            }
                        } else {
                            let spans = self.inline(v);
                            self.push(kind, ctx, spans);
                        }
                    }
                }
                "text-author" => {
                    let spans = self.inline(e);
                    self.push(Kind::TextAuthor, ctx, spans);
                }
                "date" => {
                    let spans = self.inline(e);
                    self.push(Kind::Date, ctx, spans);
                }
                _ => {}
            }
        }
    }

    /// Таблица пока выводится строками с разделителями ячеек (M2: настоящая таблица).
    fn table(&mut self, t: &Element, ctx: Context) {
        for tr in t.elements().filter(|e| e.name == "tr") {
            let mut spans = Vec::new();
            for (i, cell) in tr.elements().filter(|e| e.name == "td" || e.name == "th").enumerate() {
                if i > 0 {
                    spans.push(Span::plain(" | "));
                }
                let bold = cell.name == "th";
                self.collect(cell, InlineStyle { bold, ..Default::default() }, None, &mut spans);
            }
            self.push(Kind::TableRow, ctx, spans);
        }
    }

    fn inline(&self, e: &Element) -> Vec<Span> {
        let mut out = Vec::new();
        self.collect(e, InlineStyle::default(), None, &mut out);
        out
    }

    fn collect(&self, e: &Element, style: InlineStyle, note: Option<&str>, out: &mut Vec<Span>) {
        for n in &e.children {
            match n {
                Node::Text(t) => out.push(Span { text: t.clone(), style, note: note.map(String::from) }),
                Node::Element(c) => {
                    let mut st = style;
                    let mut nt = note.map(String::from);
                    match c.name.as_str() {
                        "emphasis" | "i" | "em" => st.italic = true,
                        "strong" | "b" => st.bold = true,
                        "strikethrough" | "s" | "strike" => st.strike = true,
                        "sup" => st.sup = true,
                        "sub" => st.sub = true,
                        "code" => st.code = true,
                        "a" => {
                            if let Some(id) = c.attr("href").and_then(|h| h.strip_prefix('#')) {
                                if c.attr("type") == Some("note") || self.notes.contains(id) {
                                    nt = Some(id.to_string());
                                    st.sup = true;
                                }
                            }
                        }
                        // Картинки внутри строки — этап M2.
                        "image" => continue,
                        _ => {}
                    }
                    self.collect(c, st, nt.as_deref(), out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOK: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><author><first-name>Тест</first-name><last-name>Корпусов</last-name></author>
<book-title>Проба</book-title><lang>ru</lang></title-info></description>
<body><section><title><p>Глава 1</p></title>
<epigraph><p>Эпиграф</p><text-author>Автор</text-author></epigraph>
<p>Текст со сноской<a l:href="#n1" type="note">1</a>.</p>
<poem><stanza><v>Строка один</v><v>Строка два</v></stanza><stanza><v>Три</v></stanza></poem>
</section><section><title><p>Глава 2</p></title><p>Ещё.</p></section></body>
<body name="notes"><section id="n1"><title><p>1</p></title><p>Сноска.</p></section></body>
</FictionBook>"##;

    #[test]
    fn structure() {
        let doc = parse(BOOK.as_bytes()).unwrap();
        assert_eq!(doc.title, "Проба");
        assert_eq!(doc.authors, vec!["Тест Корпусов"]);
        assert_eq!(doc.toc.len(), 2);
        assert_eq!(doc.notes["n1"][0].text(), "Сноска.");
        let kinds: Vec<&Kind> = doc.blocks.iter().map(|b| &b.kind).collect();
        assert!(kinds.contains(&&Kind::StanzaGap));
        assert!(kinds.contains(&&Kind::PageBreak));
        let para = doc.blocks.iter().find(|b| b.text().starts_with("Текст")).unwrap();
        assert!(para.spans.iter().any(|s| s.note.as_deref() == Some("n1") && s.style.sup));
        assert!(doc.blocks.iter().any(|b| b.ctx.epigraph && b.kind == Kind::TextAuthor));
    }

    #[test]
    fn cp1251() {
        let src = "<?xml version=\"1.0\" encoding=\"windows-1251\"?><FictionBook><body><section><p>Ёлка</p></section></body></FictionBook>";
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(src);
        let doc = parse(&bytes).unwrap();
        assert_eq!(doc.blocks.last().unwrap().text(), "Ёлка");
    }
}
