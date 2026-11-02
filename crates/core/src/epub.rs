//! Разбор EPUB 2 и 3 в общую модель документа (ту же, что у FB2).
//!
//! Порядок: `container.xml` → OPF (метаданные, манифест, spine) → файлы XHTML
//! в порядке spine → блоки. Каждый файл spine начинается с новой страницы.
//! Нелинейные файлы (`linear="no"`: обложка, примечания) в поток не попадают,
//! но из них берутся сноски. Оглавление — из `nav` (EPUB 3) или NCX (EPUB 2),
//! иначе из заголовков.
//!
//! Сноски: ссылка `epub:type="noteref"` или короткая ссылка вида «1», «*», «[2]»
//! на элемент с id (EPUB 2 после Calibre). Текст сноски — блок с этим id;
//! `aside` со сноской в поток не попадает, ссылки «назад» внутри сноски убираются.

use crate::css::{self, Info, Sheet};
use crate::fb2::decode;
use crate::model::{Block, BlockCss, Context, Document, InlineStyle, Kind, Lang, Span, TocEntry};
use crate::para::Align;
use crate::typo;
use crate::xml::{self, Element, Node};
use crate::zip::{self, Entry};
use std::collections::{HashMap, HashSet};

pub fn is_epub(entries: &[Entry]) -> bool {
    entries.iter().any(|e| e.name == "META-INF/container.xml")
}

struct Archive<'a> {
    bytes: &'a [u8],
    entries: &'a [Entry],
}

impl Archive<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let e = self
            .entries
            .iter()
            .find(|e| e.name == path)
            .or_else(|| self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(path)))?;
        zip::read(self.bytes, e).ok()
    }

    fn text(&self, path: &str) -> Option<String> {
        self.read(path).map(|b| decode(&b))
    }
}

fn dir_of(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i + 1])
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Путь `href` относительно папки `base` → путь внутри архива.
fn resolve(base: &str, href: &str) -> String {
    let mut parts: Vec<String> = base.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
    for seg in percent_decode(href).split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s.to_string()),
        }
    }
    parts.join("/")
}

/// Ссылка из файла `from` → ключ «путь#якорь» (или просто «путь»).
fn target(from: &str, href: &str) -> Option<String> {
    if href.contains("://") || href.starts_with("mailto:") {
        return None;
    }
    let (path, frag) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    let path = if path.is_empty() { from.to_string() } else { resolve(dir_of(from), path) };
    Some(match frag {
        Some(f) if !f.is_empty() => format!("{path}#{}", percent_decode(f)),
        _ => path,
    })
}

fn classes(e: &Element) -> Vec<String> {
    e.attr("class").map(|c| c.split_whitespace().map(|s| s.to_ascii_lowercase()).collect()).unwrap_or_default()
}

fn info(e: &Element) -> Info {
    Info { tag: e.name.clone(), classes: classes(e), id: e.attr("id").map(String::from) }
}

fn has_class(e: &Element, words: &[&str]) -> bool {
    classes(e).iter().any(|c| words.iter().any(|w| c.contains(w)))
}

fn epub_type(e: &Element) -> String {
    e.attr("type").unwrap_or_default().to_ascii_lowercase()
}

fn is_note_aside(e: &Element) -> bool {
    let t = epub_type(e);
    e.name == "aside" && ["footnote", "endnote", "rearnote", "note"].iter().any(|k| t.contains(k))
}

/// Похоже на знак сноски: «1», «12», «*», «†», «[3]», «(4)».
fn is_note_label(text: &str) -> bool {
    let t = text.trim().trim_matches(['[', ']', '(', ')', '{', '}']);
    !t.is_empty() && t.chars().count() <= 4 && t.chars().all(|c| c.is_ascii_digit() || "*†‡§¶".contains(c))
}

const BLOCK_TAGS: &[&str] = &[
    "p", "div", "section", "article", "aside", "header", "footer", "main", "nav", "blockquote", "h1", "h2", "h3", "h4",
    "h5", "h6", "ul", "ol", "li", "dl", "dt", "dd", "table", "thead", "tbody", "tfoot", "tr", "pre", "figure",
    "figcaption", "hr", "center", "address", "body", "hgroup", "details", "summary",
];

fn is_block(e: &Element) -> bool {
    BLOCK_TAGS.contains(&e.name.as_str())
}

/// Строчный элемент, внутри которого есть абзацы (`<span>` вокруг всей главы у
/// конвертеров из FB2), — на деле контейнер: иначе глава слилась бы в один абзац.
fn contains_block(e: &Element) -> bool {
    e.elements().any(|c| is_block(c) || contains_block(c))
}

/// Элемент с id и цепочка его предков (от корня).
fn find_id<'e>(e: &'e Element, id: &str, path: &mut Vec<&'e Element>) -> bool {
    path.push(e);
    if e.attr("id") == Some(id) || e.attr("name") == Some(id) {
        return true;
    }
    for c in e.elements() {
        if find_id(c, id, path) {
            return true;
        }
    }
    path.pop();
    false
}

struct Doc {
    path: String,
    root: Element,
    linear: bool,
    sheet: Sheet,
}

/// Ссылки на сноски: какие якоря — тексты сносок, какие `a` — сами ссылки.
#[derive(Default)]
struct Notes {
    targets: HashSet<String>,
    /// «путь#id» элементов-ссылок: ссылка на них из сноски — это «назад».
    refs: HashSet<String>,
}

impl Notes {
    fn scan(docs: &HashMap<String, Doc>) -> Notes {
        let mut n = Notes::default();
        for d in docs.values() {
            n.scan_el(&d.root, d, docs);
        }
        n
    }

    fn scan_el(&mut self, e: &Element, d: &Doc, docs: &HashMap<String, Doc>) {
        for c in e.elements() {
            if c.name == "a" {
                if let Some(key) = note_target(c, &d.path, docs) {
                    self.targets.insert(key);
                    if let Some(id) = c.attr("id") {
                        self.refs.insert(format!("{}#{id}", d.path));
                    }
                }
            }
            self.scan_el(c, d, docs);
        }
    }
}

/// Если `a` — ссылка на сноску, ключ её текста «путь#id».
fn note_target(a: &Element, from: &str, docs: &HashMap<String, Doc>) -> Option<String> {
    let key = target(from, a.attr("href")?)?;
    let (path, id) = key.split_once('#')?;
    let explicit = epub_type(a).contains("noteref");
    if !explicit && !is_note_label(&a.text()) {
        return None;
    }
    let mut chain = Vec::new();
    if !find_id(&docs.get(path)?.root, id, &mut chain) {
        return None;
    }
    // Ссылка на другую ссылку — это возврат из сноски к тексту, а не сноска.
    let el = chain.last()?;
    if !explicit && el.name == "a" && el.attr("href").is_some() {
        return None;
    }
    Some(key)
}

pub fn parse(bytes: &[u8], entries: &[Entry]) -> Result<Document, String> {
    let ar = Archive { bytes, entries };
    let container = xml::parse(&ar.text("META-INF/container.xml").ok_or("нет META-INF/container.xml")?);
    let opf_path = container.find("rootfile").and_then(|r| r.attr("full-path")).ok_or("нет пути к OPF")?.to_string();
    let opf = xml::parse(&ar.text(&opf_path).ok_or_else(|| format!("нет файла {opf_path}"))?);
    let base = dir_of(&opf_path).to_string();

    let meta = opf.find("metadata");
    let lang = meta.and_then(|m| m.find("language")).map_or(Lang::Ru, |l| Lang::from_code(&l.text()));
    let mut doc = Document { lang, ..Default::default() };
    if let Some(m) = meta {
        doc.title = collapse(&m.find("title").map(|t| t.text()).unwrap_or_default());
        doc.authors = m.elements().filter(|e| e.name == "creator").map(|e| collapse(&e.text())).collect();
    }

    // Манифест: id → (путь, свойства).
    let mut manifest: HashMap<String, (String, String)> = HashMap::new();
    let mut nav_path = None;
    if let Some(man) = opf.find("manifest") {
        for it in man.elements().filter(|e| e.name == "item") {
            let (Some(id), Some(href)) = (it.attr("id"), it.attr("href")) else { continue };
            let path = resolve(&base, href);
            let props = it.attr("properties").unwrap_or_default().to_string();
            if props.split_whitespace().any(|p| p == "nav") {
                nav_path = Some(path.clone());
            }
            manifest.insert(id.to_string(), (path, props));
        }
    }
    let spine = opf.find("spine").ok_or("в OPF нет spine")?;
    let ncx_path = spine.attr("toc").and_then(|id| manifest.get(id)).map(|(p, _)| p.clone());

    let mut css_cache: HashMap<String, String> = HashMap::new();
    let mut order = Vec::new();
    let mut docs: HashMap<String, Doc> = HashMap::new();
    for ir in spine.elements().filter(|e| e.name == "itemref") {
        let Some((path, _)) = ir.attr("idref").and_then(|id| manifest.get(id)) else { continue };
        if docs.contains_key(path) {
            continue;
        }
        let Some(text) = ar.text(path) else { continue };
        let root = xml::parse(&text);
        let sheet = stylesheets(&root, path, &ar, &mut css_cache);
        let linear = ir.attr("linear") != Some("no");
        order.push(path.clone());
        docs.insert(path.clone(), Doc { path: path.clone(), root, linear, sheet });
    }

    let notes = Notes::scan(&docs);
    let mut conv = Conv::new(lang, &notes, false);
    for path in &order {
        let d = &docs[path];
        if !d.linear {
            continue;
        }
        conv.page_break();
        conv.anchors.insert(path.clone(), conv.blocks.len());
        conv.path = path.clone();
        conv.sheet = d.sheet.clone();
        if let Some(body) = d.root.find("body") {
            let st = St { lang: lang_of(body).unwrap_or(lang), ..St::default() };
            conv.block(body, st);
        }
    }

    // Тексты сносок.
    for key in &notes.targets {
        let Some((path, id)) = key.split_once('#') else { continue };
        let Some(d) = docs.get(path) else { continue };
        let mut chain = Vec::new();
        if !find_id(&d.root, id, &mut chain) {
            continue;
        }
        // Сам элемент с абзацами внутри (<span id> у конвертеров из FB2) — это и есть сноска;
        // якорь внутри строки — берём ближайший блок вокруг него.
        let Some(el) = chain
            .iter()
            .rev()
            .find(|e| e.name != "body" && (is_block(e) || contains_block(e)))
            .copied()
        else {
            continue;
        };
        let mut nc = Conv::new(lang, &notes, true);
        nc.path = path.to_string();
        nc.sheet = d.sheet.clone();
        let st = St { lang: chain.iter().rev().find_map(|e| lang_of(e)).unwrap_or(lang), ..St::default() };
        nc.block(el, st);
        // Первый блок из одного номера сноски («7», «[7]») — её заголовок, знак уже в тексте.
        if nc.blocks.first().is_some_and(|b| is_note_label(&b.text())) {
            nc.blocks.remove(0);
        }
        if !nc.blocks.is_empty() {
            doc.notes.insert(key.clone(), nc.blocks);
        }
    }

    doc.toc = toc(&ar, nav_path.as_deref(), ncx_path.as_deref(), &conv.anchors);
    if doc.toc.is_empty() {
        doc.toc = conv.headings.clone();
    }
    doc.blocks = conv.blocks;
    mark_styled_subtitles(&mut doc.blocks);
    mark_italic_epigraphs(&mut doc.blocks);
    Ok(doc)
}

/// Издатель может обозначить подзаголовок только центровкой и жирными прописными,
/// в том числе во вложенных span/em, без увеличенного font-size и тегов h1–h6.
fn mark_styled_subtitles(blocks: &mut [Block]) {
    for b in blocks {
        if b.kind != Kind::Para || b.ctx != Context::default() || b.css.align != Some(Align::Center) {
            continue;
        }
        let text = b.text();
        let t = text.trim();
        let end = t.trim_end_matches(['»', '”', '"', '\'', '’', ')', ']']);
        if t.chars().count() <= 100
            && t.chars().any(char::is_uppercase)
            && !t.chars().any(char::is_lowercase)
            && !end.ends_with(['.', '!', '?', '…', ':', ';', ','])
            && b.spans.iter().filter(|s| !s.text.trim().is_empty()).all(|s| {
                let st = s.style;
                s.note.is_none() && st.bold && !st.sup && !st.sub && !st.code && !st.strike
            })
        {
            b.kind = Kind::Subtitle;
        }
    }
}

/// Несколько коротких абзацев подряд целиком курсивом — вступление-эпиграф, которое издатель
/// не разметил (в EPUB это обычные `<p><em>`): ставим его как эпиграф. Одиночный курсивный
/// абзац — мысль героя, его не трогаем.
fn mark_italic_epigraphs(blocks: &mut [Block]) {
    let italic = |b: &Block| {
        b.kind == Kind::Para
            && b.ctx == Context::default()
            && b.spans.iter().all(|s| s.note.is_none() && (s.style.italic || s.text.trim().is_empty() || !s.text.chars().any(char::is_alphanumeric)))
            && b.spans.iter().any(|s| s.style.italic && s.text.chars().any(char::is_alphanumeric))
            && b.text().chars().count() <= 200
            && !b.text().trim_start().starts_with(['—', '–', '-'])
    };
    let mut i = 0;
    while i < blocks.len() {
        let mut j = i;
        while j < blocks.len() && italic(&blocks[j]) {
            j += 1;
        }
        if (2..=6).contains(&(j - i)) {
            for b in &mut blocks[i..j] {
                b.ctx.epigraph = true;
                // Эпиграф сам набирается курсивом; курсив внутри него вернул бы прямое начертание.
                for sp in &mut b.spans {
                    sp.style.italic = false;
                }
            }
        }
        i = j.max(i + 1);
    }
}

fn stylesheets(root: &Element, path: &str, ar: &Archive, cache: &mut HashMap<String, String>) -> Sheet {
    let mut sheet = Sheet::default();
    let Some(head) = root.find("head") else { return sheet };
    for e in head.elements() {
        if e.name == "link" && e.attr("rel").is_some_and(|r| r.to_ascii_lowercase().contains("stylesheet")) {
            let Some(href) = e.attr("href") else { continue };
            let p = resolve(dir_of(path), href);
            let css = cache.entry(p.clone()).or_insert_with(|| ar.text(&p).unwrap_or_default());
            sheet.add(css);
        } else if e.name == "style" {
            sheet.add(&e.text());
        }
    }
    sheet
}

fn lang_of(e: &Element) -> Option<Lang> {
    e.attr("lang").filter(|l| !l.is_empty()).map(Lang::from_code)
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Оглавление из nav (EPUB 3) или NCX (EPUB 2).
fn toc(ar: &Archive, nav: Option<&str>, ncx: Option<&str>, anchors: &HashMap<String, usize>) -> Vec<TocEntry> {
    // (файл оглавления, ссылка, название, уровень)
    let mut raw: Vec<(String, String, String, u8)> = Vec::new();
    if let Some(path) = nav {
        if let Some(root) = ar.text(path).map(|t| xml::parse(&t)) {
            let navs: Vec<&Element> = all(&root, "nav");
            let nav = navs.iter().find(|n| epub_type(n).contains("toc")).or(navs.first());
            if let Some(ol) = nav.and_then(|n| n.find("ol")) {
                nav_list(ol, 1, &mut |href, title, level| raw.push((path.into(), href.into(), title, level)));
            }
        }
    }
    if raw.is_empty() {
        if let Some(path) = ncx {
            if let Some(map) = ar.text(path).map(|t| xml::parse(&t)).as_ref().and_then(|r| r.find("navmap")) {
                ncx_points(map, 1, &mut |href, title, level| raw.push((path.into(), href.into(), title, level)));
            }
        }
    }
    raw.into_iter()
        .filter(|(_, _, title, _)| !title.is_empty())
        .filter_map(|(from, href, title, level)| {
            let key = target(&from, &href)?;
            let block = anchors.get(&key).or_else(|| anchors.get(key.split('#').next().unwrap_or_default()))?;
            Some(TocEntry { title, level, block: *block })
        })
        .collect()
}

fn all<'e>(e: &'e Element, name: &str) -> Vec<&'e Element> {
    let mut out = Vec::new();
    for c in e.elements() {
        if c.name == name {
            out.push(c);
        }
        out.extend(all(c, name));
    }
    out
}

fn nav_list(ol: &Element, level: u8, add: &mut dyn FnMut(&str, String, u8)) {
    for li in ol.elements().filter(|e| e.name == "li") {
        if let Some(a) = li.find("a") {
            if let Some(href) = a.attr("href") {
                add(href, collapse(&a.text()), level);
            }
        }
        if let Some(sub) = li.elements().find(|e| e.name == "ol") {
            nav_list(sub, level.saturating_add(1), add);
        }
    }
}

fn ncx_points(e: &Element, level: u8, add: &mut dyn FnMut(&str, String, u8)) {
    for np in e.elements().filter(|e| e.name == "navpoint") {
        let title = np.child("navlabel").map(|l| collapse(&l.text())).unwrap_or_default();
        if let Some(src) = np.child("content").and_then(|c| c.attr("src")) {
            add(src, title, level);
        }
        ncx_points(np, level.saturating_add(1), add);
    }
}

/// Наследуемое состояние при обходе.
#[derive(Clone, Copy, Default)]
struct St {
    style: InlineStyle,
    lang: Lang,
    ctx: Context,
    align: Option<Align>,
    indent: Option<f32>,
    left: css::Len,
    right: css::Len,
    /// Уровень вложенности списка и нумерованный ли он.
    list: Option<(u8, bool)>,
    /// Внутри блока-заголовка (`<div class="title">` с крупным жирным шрифтом):
    /// его абзацы — строки заголовка. Уровень; 0 — подзаголовок.
    heading: Option<u8>,
}

/// Строки абзаца: `<br/>` начинает новую.
#[derive(Default)]
struct Buf {
    lines: Vec<Vec<Span>>,
}

impl Buf {
    fn text(&mut self, t: &str, style: InlineStyle, note: Option<&str>) {
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        let t: String = t.chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect();
        let line = self.lines.last_mut().expect("строка есть");
        match line.last_mut() {
            Some(s) if s.style == style && s.note.as_deref() == note => s.text.push_str(&t),
            _ => line.push(Span { text: t, style, note: note.map(String::from) }),
        }
    }

    fn br(&mut self) {
        self.lines.push(Vec::new());
    }
}

struct Conv<'a> {
    lang: Lang,
    notes: &'a Notes,
    in_note: bool,
    path: String,
    sheet: Sheet,
    chain: Vec<Info>,
    blocks: Vec<Block>,
    anchors: HashMap<String, usize>,
    /// Заголовки — запасное оглавление, если в книге его нет.
    headings: Vec<TocEntry>,
}

impl<'a> Conv<'a> {
    fn new(lang: Lang, notes: &'a Notes, in_note: bool) -> Self {
        Conv {
            lang,
            notes,
            in_note,
            path: String::new(),
            sheet: Sheet::default(),
            chain: Vec::new(),
            blocks: Vec::new(),
            anchors: HashMap::new(),
            headings: Vec::new(),
        }
    }

    fn page_break(&mut self) {
        if !self.in_note && !matches!(self.blocks.last().map(|b| &b.kind), None | Some(Kind::PageBreak)) {
            self.mark(Kind::PageBreak, Context::default());
        }
    }

    fn mark(&mut self, kind: Kind, ctx: Context) {
        self.blocks.push(Block { kind, ctx, spans: Vec::new(), lang: self.lang, css: BlockCss::default() });
    }

    fn push(&mut self, kind: Kind, st: &St, mut spans: Vec<Span>, css: BlockCss) {
        // Код — как есть: типограф снял бы отступы строк.
        if kind != Kind::Pre {
            typo::apply(&mut spans, st.lang);
        }
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }
        // Заголовок сноски — её номер («7»): в попапе он не нужен, знак уже в тексте.
        if self.in_note && matches!(kind, Kind::Heading(_) | Kind::Subtitle) {
            return;
        }
        if let Kind::Heading(level) = kind {
            let block = self.blocks.len();
            let title: String = spans.iter().map(|s| s.text.as_str()).collect();
            // Заголовок из нескольких строк — один пункт оглавления.
            match self.headings.last_mut() {
                Some(h) if h.block + 1 == block && h.level == level && self.blocks.last().map(|b| &b.kind) == Some(&kind) => {
                    h.title.push(' ');
                    h.title.push_str(&title);
                }
                _ => self.headings.push(TocEntry { title, level, block }),
            }
        }
        self.blocks.push(Block { kind, ctx: st.ctx, spans, lang: st.lang, css });
    }

    /// CSS элемента + наследуемое состояние. `None` — элемент скрыт.
    fn enter(&mut self, e: &Element, parent: &St) -> Option<(St, HashMap<String, String>)> {
        if e.attr("hidden").is_some() {
            return None;
        }
        self.chain.push(info(e));
        let d = self.sheet.computed(&self.chain, e.attr("style"));
        if d.get("display").is_some_and(|v| v == "none") {
            self.chain.pop();
            return None;
        }
        if let Some(id) = e.attr("id") {
            self.anchors.entry(format!("{}#{id}", self.path)).or_insert(self.blocks.len());
        }
        let mut st = *parent;
        if let Some(l) = lang_of(e) {
            st.lang = l;
        }
        match e.name.as_str() {
            "em" | "i" | "cite" | "var" | "dfn" => st.style.italic = true,
            "strong" | "b" => st.style.bold = true,
            "sup" => st.style.sup = true,
            "sub" => st.style.sub = true,
            "code" | "tt" | "kbd" | "samp" => st.style.code = true,
            "s" | "strike" | "del" => st.style.strike = true,
            _ => {}
        }
        match d.get("font-style").map(String::as_str) {
            Some("italic" | "oblique") => st.style.italic = true,
            Some("normal") => st.style.italic = false,
            _ => {}
        }
        if let Some(w) = d.get("font-weight") {
            match w.as_str() {
                "bold" | "bolder" | "600" | "700" | "800" | "900" => st.style.bold = true,
                "normal" | "lighter" | "100" | "200" | "300" | "400" => st.style.bold = false,
                _ => {}
            }
        }
        match d.get("vertical-align").map(String::as_str) {
            Some("super" | "sup") => st.style.sup = true,
            Some("sub") => st.style.sub = true,
            _ => {}
        }
        if is_block(e) {
            match d.get("text-align").map(String::as_str) {
                Some("center") => st.align = Some(Align::Center),
                Some("right" | "end") => st.align = Some(Align::Right),
                Some(_) => st.align = None,
                None => {}
            }
            if let Some(len) = d.get("text-indent").and_then(|v| css::length(v)) {
                st.indent = Some(len.em);
            }
            // Поля корня издателя и контейнеров, которым мы сами даём отступ, не берём.
            let own = !matches!(e.name.as_str(), "body" | "html" | "blockquote")
                && !has_class(e, &["epigraph", "poem", "stanza", "verse"]);
            if own {
                let (l, r) = css::margins(&d);
                st.left = css::Len { em: st.left.em + l.em, pct: st.left.pct + l.pct };
                st.right = css::Len { em: st.right.em + r.em, pct: st.right.pct + r.pct };
            }
        }
        Some((st, d))
    }

    fn block_css(st: &St) -> BlockCss {
        BlockCss {
            align: st.align,
            indent: st.indent,
            left_em: st.left.em,
            left_pct: st.left.pct,
            right_em: st.right.em,
            right_pct: st.right.pct,
        }
    }

    /// Блочный элемент.
    fn block(&mut self, e: &Element, parent: St) {
        if !self.in_note && is_note_aside(e) {
            return;
        }
        if matches!(e.name.as_str(), "img" | "svg" | "image" | "script" | "head" | "title") {
            return;
        }
        let Some((mut st, d)) = self.enter(e, &parent) else { return };
        let brk = |k: &str| d.get(k).is_some_and(|v| v == "always" || v == "page" || v == "left" || v == "right");
        if brk("page-break-before") || brk("break-before") {
            self.page_break();
        }
        if e.name == "blockquote" {
            st.ctx.cite = true;
        }
        if has_class(e, &["epigraph"]) {
            st.ctx.epigraph = true;
        }
        if has_class(e, &["poem", "stanza", "verse"]) {
            st.ctx.poem = true;
        }
        match e.name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = e.name[1..].parse::<u8>().unwrap_or(1);
                self.container(e, st, Kind::Heading(level));
            }
            "pre" => self.pre(e, st),
            "table" => self.table(e, st),
            "hr" => self.mark(Kind::EmptyLine, st.ctx),
            "ul" | "ol" => {
                let depth = st.list.map_or(1, |(d, _)| d + 1);
                st.list = Some((depth, e.name == "ol"));
                let mut n = 0;
                for c in e.elements() {
                    if c.name == "li" {
                        n += 1;
                        self.list_item(c, st, n);
                    } else {
                        self.block(c, st);
                    }
                }
            }
            "li" => self.list_item(e, st, 1),
            "figcaption" => {
                st.align = Some(Align::Center);
                st.indent = Some(0.0);
                st.style.italic = true;
                self.container(e, st, Kind::Para);
            }
            _ => {
                // Подпись под эпиграфом или цитатой — автор, как <text-author> в FB2.
                let author = (st.ctx.epigraph || st.ctx.cite)
                    && (has_class(e, &["author", "attribution", "signature", "sign"]) || st.align == Some(Align::Right));
                let own_heading = heading_like(&d);
                if e.elements().any(is_block) {
                    match own_heading {
                        Some(Kind::Heading(l)) => st.heading = Some(l),
                        Some(Kind::Subtitle) => st.heading = Some(0),
                        _ => {}
                    }
                }
                let empty = e.children.iter().all(|n| matches!(n, Node::Text(t) if t.trim().is_empty()));
                if empty && has_class(e, &["empty"]) {
                    // Пустая строка из FB2 после конвертера: <p class="empty-line"/>.
                    self.mark(Kind::EmptyLine, st.ctx);
                    self.chain.pop();
                    return;
                }
                let kind = if author {
                    Kind::TextAuthor
                } else if let Some(l) = st.heading {
                    if l == 0 { Kind::Subtitle } else { Kind::Heading(l) }
                } else if st.ctx.poem && e.name == "p" {
                    Kind::Verse
                } else {
                    own_heading.unwrap_or(Kind::Para)
                };
                self.container(e, st, kind);
                if has_class(e, &["stanza"]) && e.name != "p" {
                    self.mark(Kind::StanzaGap, st.ctx);
                }
            }
        }
        if brk("page-break-after") || brk("break-after") {
            self.page_break();
        }
        self.chain.pop();
    }

    fn list_item(&mut self, li: &Element, list_st: St, n: u32) {
        let Some((mut st, _)) = self.enter(li, &list_st) else { return };
        let (depth, ordered) = st.list.unwrap_or((1, false));
        st.left.em += 1.5 * depth as f32;
        st.indent = Some(-1.2);
        let prefix = if ordered { format!("{n}.\u{a0}") } else { "•\u{a0}".to_string() };
        let mut buf = Buf::default();
        buf.text(&prefix, st.style, None);
        self.flow(li, st, Kind::Para, buf);
        self.chain.pop();
    }

    /// Содержимое блока: текст и строчные элементы собираются в абзац,
    /// вложенные блоки прерывают его.
    fn container(&mut self, e: &Element, st: St, kind: Kind) {
        self.flow(e, st, kind, Buf::default());
    }

    fn flow(&mut self, e: &Element, st: St, kind: Kind, mut buf: Buf) {
        for n in &e.children {
            match n {
                Node::Text(t) => buf.text(t, st.style, None),
                Node::Element(c) if is_block(c) || contains_block(c) => {
                    self.flush(&mut buf, &kind, &st);
                    self.block(c, st);
                }
                Node::Element(c) => self.inline(c, &st, &mut buf),
            }
        }
        self.flush(&mut buf, &kind, &st);
    }

    fn flush(&mut self, buf: &mut Buf, kind: &Kind, st: &St) {
        let lines = std::mem::take(&mut buf.lines);
        let verse = *kind == Kind::Verse;
        for (i, line) in lines.into_iter().enumerate() {
            let mut css = Self::block_css(st);
            // Строка после <br/> — продолжение абзаца, без красной строки.
            if i > 0 && !verse && !matches!(kind, Kind::Heading(_)) {
                css.indent = Some(0.0);
            }
            let k = if st.ctx.poem && i > 0 && *kind == Kind::Para { Kind::Verse } else { kind.clone() };
            // Эпиграф и подпись движок сам ставит курсивом, а выделение внутри курсива — прямое.
            // Если издатель набрал курсивом всю строку, снимаем его, чтобы курсив не погас.
            let mut line = line;
            let italic_block = matches!(k, Kind::TextAuthor | Kind::Date) || (st.ctx.epigraph && k == Kind::Para);
            if italic_block && line.iter().filter(|s| !s.text.trim().is_empty()).all(|s| s.style.italic) {
                line.iter_mut().for_each(|s| s.style.italic = false);
            }
            self.push(k, st, line, css);
        }
    }

    fn inline(&mut self, e: &Element, parent: &St, buf: &mut Buf) {
        match e.name.as_str() {
            "br" => {
                buf.br();
                return;
            }
            "img" | "image" | "svg" | "script" | "style" => return,
            _ => {}
        }
        if epub_type(e).contains("pagebreak") {
            return;
        }
        let Some((st, _)) = self.enter(e, parent) else { return };
        let mut note: Option<String> = None;
        if e.name == "a" {
            if let Some(key) = e.attr("href").and_then(|h| target(&self.path, h)) {
                // Внутри сноски ссылка на знак сноски — это «вернуться», её не показываем.
                if self.in_note && self.notes.refs.contains(&key) {
                    self.chain.pop();
                    return;
                }
                if self.notes.targets.contains(&key) && !self.in_note {
                    note = Some(key);
                }
            }
        }
        let mut st = st;
        if note.is_some() {
            st.style.sup = true;
        }
        self.inline_children(e, &st, buf, note.as_deref());
        self.chain.pop();
    }

    fn inline_children(&mut self, e: &Element, st: &St, buf: &mut Buf, note: Option<&str>) {
        for n in &e.children {
            match n {
                Node::Text(t) => buf.text(t, st.style, note),
                Node::Element(c) if note.is_some() => {
                    // Внутри знака сноски: <sup>2</sup> и подобное — тот же знак.
                    let mut inner = *st;
                    if let Some((s, _)) = self.enter(c, st) {
                        inner.style = s.style;
                        self.inline_children(c, &inner, buf, note);
                        self.chain.pop();
                    }
                }
                Node::Element(c) => self.inline(c, st, buf),
            }
        }
    }

    fn pre(&mut self, e: &Element, st: St) {
        let text = e.text();
        let text = text.strip_prefix('\n').unwrap_or(&text).trim_end();
        let style = InlineStyle { code: true, ..st.style };
        for line in text.split('\n') {
            let line = line.trim_end();
            if line.is_empty() {
                self.mark(Kind::EmptyLine, st.ctx);
                continue;
            }
            // Отступ кода держат неразрывные пробелы: обычные съела бы выключка.
            let lead = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            let body: String = line.chars().skip(lead).collect();
            let text = format!("{}{body}", "\u{a0}".repeat(lead));
            self.push(Kind::Pre, &st, vec![Span { text, style, note: None }], Self::block_css(&st));
        }
    }

    /// Таблица пока строками с разделителем ячеек, как в FB2.
    fn table(&mut self, t: &Element, st: St) {
        for tr in all(t, "tr") {
            let mut buf = Buf::default();
            for (i, cell) in tr.elements().filter(|e| e.name == "td" || e.name == "th").enumerate() {
                if i > 0 {
                    buf.text(" | ", st.style, None);
                }
                let mut cs = st;
                cs.style.bold |= cell.name == "th";
                self.inline_children(cell, &cs, &mut buf, None);
            }
            let line: Vec<Span> = buf.lines.into_iter().flatten().collect();
            self.push(Kind::TableRow, &st, line, BlockCss::default());
        }
    }
}

/// `<p class="title">` у Calibre: крупный жирный или центрированный текст — заголовок.
fn heading_like(d: &HashMap<String, String>) -> Option<Kind> {
    let size = d.get("font-size").and_then(|v| match v.as_str() {
        "xx-large" => Some(2.0),
        "x-large" => Some(1.5),
        "large" | "larger" => Some(1.2),
        v => css::length(v).map(|l| if l.pct > 0.0 { l.pct } else { l.em }),
    })?;
    let bold = d.get("font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600));
    let center = d.get("text-align").is_some_and(|a| a == "center");
    if size >= 1.3 && (bold || center) {
        Some(Kind::Heading(if size >= 1.5 { 1 } else { 2 }))
    } else if size >= 1.05 && bold && center {
        Some(Kind::Subtitle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book;

    fn corpus(name: &str) -> Document {
        let b = std::fs::read(format!("{}/../../tests/corpus/synthetic/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        book::parse(&b).unwrap()
    }

    fn texts(d: &Document) -> Vec<String> {
        d.blocks.iter().map(|b| b.text()).collect()
    }

    #[test]
    fn epub3_english() {
        let d = corpus("06_english_epub3.epub");
        assert_eq!(d.title, "06 English EPUB3");
        assert_eq!(d.lang, Lang::En);
        let t = texts(&d);
        let all = t.join("\n");
        // Сноска aside — не в потоке, а в попапе; скрытый абзац не виден.
        assert!(!all.contains("This is a footnote inside"));
        assert!(!all.contains("hidden by CSS"));
        assert!(!all.contains("cover"), "обложка linear=no не в потоке");
        let fn1 = d.notes.iter().find(|(k, _)| k.ends_with("#fn1")).expect("сноска fn1");
        assert!(fn1.1[0].text().starts_with("This is a footnote"));
        let en1 = d.notes.iter().find(|(k, _)| k.ends_with("#en1")).expect("концевая сноска en1");
        assert!(!en1.1.iter().any(|b| b.text().contains('↩')), "ссылка назад убрана");
        // Знаки сносок — ссылки с верхним индексом.
        let refs: Vec<&Span> = d.blocks.iter().flat_map(|b| &b.spans).filter(|s| s.note.is_some()).collect();
        assert!(refs.len() >= 3 && refs.iter().all(|s| s.style.sup));
        // Английская типографика и русский абзац внутри.
        assert!(all.contains("“Well,” he said, “I don’t think it’s possible—not in the ’90s, anyway.”"));
        let ru = d.blocks.iter().find(|b| b.text().starts_with("Абзац на\u{a0}русском")).unwrap();
        assert_eq!(ru.lang, Lang::Ru);
        assert!(ru.text().contains("«кавычки»"));
        // Оглавление из nav с вложенностью.
        let toc: Vec<(u8, &str)> = d.toc.iter().map(|t| (t.level, t.title.as_str())).collect();
        assert_eq!(toc[0], (1, "Chapter One: The Letter"));
        assert!(toc.contains(&(2, "Jabberwocky")));
        // Стихи через классы, pre построчно и с отступом.
        assert!(d.blocks.iter().any(|b| b.kind == Kind::Verse && b.text().contains("brillig")));
        let pre: Vec<String> = d.blocks.iter().filter(|b| b.kind == Kind::Pre).map(|b| format!("{:?}", b.text())).collect();
        assert!(d.blocks.iter().any(|b| b.kind == Kind::Pre && b.text().starts_with("\u{a0}\u{a0}\u{a0}\u{a0}println!")), "{pre:?}");
        assert!(d.blocks.iter().any(|b| b.text().starts_with("•\u{a0}First item")));
        assert!(d.blocks.iter().any(|b| b.ctx.cite));
    }

    #[test]
    fn epub2_calibre() {
        let d = corpus("07_epub2_calibre.epub");
        assert_eq!(d.lang, Lang::Ru);
        assert_eq!(d.authors, vec!["Тест Корпусов"]);
        let toc: Vec<&str> = d.toc.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(toc, ["Глава 1", "Глава 2", "Вложенный пункт", "Глава 3"]);
        // <p class="title"> — заголовок; курсив и жирный классами; сущности.
        assert!(d.blocks.iter().any(|b| matches!(b.kind, Kind::Heading(_)) && b.text() == "Глава 1"));
        let p = d.blocks.iter().find(|b| b.text().starts_with("Курсив задан")).unwrap();
        assert!(p.spans.iter().any(|s| s.style.italic && s.text.contains("Курсив")));
        assert!(p.spans.iter().any(|s| s.style.bold && s.text.contains("Жирный")));
        assert!(p.text().contains("«ёлочки»"));
        // Сноска EPUB2 без epub:type найдена по короткой ссылке.
        let n = d.notes.values().next().expect("сноска");
        assert!(n[0].text().contains("Текст сноски EPUB2"));
        assert!(!n[0].text().starts_with('1'), "ссылка назад «1» убрана");
        // Стихи через <br/>: отдельные строки.
        assert!(d.blocks.iter().any(|b| b.text() == "В\u{a0}тумане моря голубом!.."));
        assert!(d.blocks.iter().any(|b| b.text().starts_with("Текст прямо в\u{a0}div")));
    }

    #[test]
    fn styled_uppercase_paragraphs_are_subtitles_without_changing_text() {
        let d = corpus("11_styled_subtitles.epub");
        let expected = ["РОЛЬ ТЕХНОЛОГИЙ", "КОУЧИНГ", "УКОМПЛЕКТОВАНИЕ ПЕРСОНАЛОМ", "ВИДЕНИЕ ПРОДУКТА"];
        let subtitles: Vec<_> = d.blocks.iter().filter(|b| b.kind == Kind::Subtitle).map(Block::text).collect();
        assert_eq!(subtitles, expected);
        for title in expected {
            let b = d.blocks.iter().find(|b| b.text() == title).unwrap();
            assert_eq!(b.ctx, Context::default());
            assert_eq!(b.css.align, Some(Align::Center));
            assert_eq!(b.css.indent, None);
            assert_eq!((b.css.left_em, b.css.right_em), (2.0, 2.0));
            assert!(b.spans.iter().all(|s| s.style.bold && s.style.italic));
        }
        // Эвристика меняет только роль блока, а не содержание, стили или оглавление.
        let root = xml::parse(include_str!("../../../tests/fixtures/epub-styled-subtitles.xhtml"));
        let notes = Notes::default();
        let mut conv = Conv::new(Lang::Ru, &notes, false);
        conv.sheet.add(&root.find("style").unwrap().text());
        conv.block(root.find("body").unwrap(), St::default());
        assert_eq!(texts(&d), conv.blocks.iter().map(Block::text).collect::<Vec<_>>());
        assert_eq!(d.toc.len(), 1);
        assert_eq!(d.toc[0].title, "Подзаголовки обычного кегля");
        for b in &d.blocks {
            if !expected.contains(&b.text().as_str()) && !matches!(b.kind, Kind::Heading(_)) {
                assert_ne!(b.kind, Kind::Subtitle, "false subtitle: {}", b.text());
            }
        }
        let body = d.blocks.iter().find(|b| b.text().starts_with("В\u{a0}большинстве")).unwrap();
        assert!(body.spans.iter().any(|s| s.text == "ю" && s.style.italic));
    }

    #[test]
    fn styled_subtitles_have_spacing_no_indent_and_stay_with_body() {
        use crate::{font::FontSet, layout::Geometry};
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus/synthetic/11_styled_subtitles.epub")).unwrap();
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fonts");
        for scroll in [false, true] {
            for bionic in [false, true] {
                let fonts = FontSet::from_prefix(&format!("{root}/PTSerif"), &format!("{root}/LiberationMono-Regular.ttf")).unwrap();
                let g = Geometry { width: 1080.0, height: 2400.0, margin_x: 88.0, margin_y: 150.0,
                    font_size: 48.250492, leading: 1.3347014, scroll, bionic };
                let book = book::Book::open(&data, fonts, g).unwrap();
                let mut checked = 0;
                for (bi, block) in book.doc.blocks.iter().enumerate().filter(|(_, b)| b.kind == Kind::Subtitle) {
                    checked += 1;
                    let st = crate::layout::block_style(block, &book.g).para;
                    assert_eq!(st.indent, 0.0);
                    assert!(!st.hyphenate);
                    assert_eq!(st.align, Align::Center);
                    let lines: Vec<_> = (0..book.page_count()).flat_map(|n| book.page(n).unwrap().lines.iter().map(move |l| (n, l))).collect();
                    let title: Vec<_> = lines.iter().filter(|(_, l)| l.block == bi).collect();
                    assert!(!title.is_empty());
                    for (_, l) in &title {
                        assert!(!l.hyphen);
                        let left = l.runs.first().unwrap().x0 + l.x;
                        let right = l.runs.last().unwrap().x1 + l.x;
                        assert!(((left + right) / 2.0 - 540.0).abs() < 0.1, "not centered: {}", block.text());
                    }
                    let (page, last) = title.last().unwrap();
                    let (next_page, next) = lines.iter().find(|(_, l)| l.block == bi + 1).unwrap();
                    assert_eq!(page, next_page, "subtitle left without body");
                    assert!(next.baseline - last.baseline >= book.g.unit() * 1.99);
                    let (first_page, first) = title.first().unwrap();
                    if let Some((prev_page, prev)) = lines.iter().rev().find(|(_, l)| l.block < bi) {
                        if first_page == prev_page {
                            assert!(first.baseline - prev.baseline >= book.g.unit() * 1.99);
                        }
                    }
                }
                assert_eq!(checked, 4);
            }
        }
    }

    #[test]
    fn calibration_matches_fb2() {
        let e = corpus("10_calibration.epub");
        let f = corpus("10_calibration.fb2");
        let body = |d: &Document| -> Vec<String> {
            d.blocks.iter().filter(|b| b.kind == Kind::Para).map(|b| b.text()).collect()
        };
        assert_eq!(body(&e), body(&f));
    }
}
