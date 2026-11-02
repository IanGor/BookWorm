//! Вертикальная вёрстка: блоки → строки на сетке → страницы.
//!
//! Основной текст стоит на сетке: каждая строка — ровно одна единица сетки.
//! Заголовки, эпиграфы и прочие блоки другого кегля дополняются до целого числа
//! единиц, поэтому последняя строка каждой полной страницы — на одной высоте.
//! Правила разбиения — docs/SPEC.md, §2.6.

use crate::font::{FontSet, REGULAR};
use crate::hyph::Hyphenator;
use crate::model::{Block, Context, Document, Kind, Lang, Position, Span};
use crate::para::{layout_paragraph, Align, GlyphRun, Line, ParaStyle};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    /// Левое и правое поле (равны).
    pub margin_x: f32,
    /// Верхнее и нижнее поле (равны; строка статуса лежит в нижнем).
    pub margin_y: f32,
    pub font_size: f32,
    /// Межстрочный интервал в долях кегля.
    pub leading: f32,
    /// Непрерывная прокрутка: глава — одна длинная полоса без разбиения на страницы.
    pub scroll: bool,
    /// Выделять начальную треть слов более плотным начертанием.
    pub bionic: bool,
}

/// Опорная высота строчных (Noto Serif, доли em): у шрифта с такой высотой
/// ступень размера равна кеглю.
pub const REF_X_HEIGHT: f32 = 0.536;

/// Ступени размера, как у Kindle: кегль опорного шрифта в px при ширине экрана 1080.
pub const SIZE_STEPS: [f32; 11] = [30.0, 34.0, 38.0, 42.0, 46.0, 51.0, 56.0, 62.0, 68.0, 76.0, 84.0];
pub const DEFAULT_STEP: usize = 4;

impl Geometry {
    /// Опорная геометрия: 1080×2400, ~402 ppi. Поля — черновые, калибруются по Kindle.
    pub fn phone(font_size: f32) -> Geometry {
        Geometry {
            width: 1080.0,
            height: 2400.0,
            margin_x: 88.0,
            margin_y: 150.0,
            font_size,
            leading: 1.4,
            scroll: false,
            bionic: false,
        }
    }

    /// Экран, ступень размера и шрифт (SPEC §2.8). Поля зависят только от экрана.
    /// Кегль подбирается так, чтобы высота строчных у любой гарнитуры была одной
    /// и той же для ступени, а шаг строк — `1,4 × ступень`: при смене шрифта
    /// плотность текста и число строк на странице не меняются.
    pub fn new(width: f32, height: f32, step: f32, x_height: f32, scroll: bool) -> Geometry {
        let k = width / 1080.0;
        let nominal = step * k;
        let font_size = nominal * REF_X_HEIGHT / x_height.clamp(0.35, 0.7);
        Geometry {
            width,
            height,
            margin_x: 88.0 * k,
            margin_y: 150.0 * k,
            font_size,
            leading: 1.4 * nominal / font_size,
            scroll,
            bionic: false,
        }
    }

    /// Кегль строки статуса: не зависит от размера текста, как у Kindle.
    pub fn footer_size(&self) -> f32 {
        27.6 * self.width / 1080.0
    }

    pub fn unit(&self) -> f32 {
        self.font_size * self.leading
    }

    pub fn text_width(&self) -> f32 {
        self.width - 2.0 * self.margin_x
    }

    pub fn lines_per_page(&self) -> usize {
        if self.scroll {
            return 1_000_000;
        }
        ((self.height - 2.0 * self.margin_y) / self.unit()).floor() as usize
    }

    /// Верх текстового блока: остаток высоты делится поровну сверху и снизу.
    pub fn text_top(&self) -> f32 {
        if self.scroll {
            return self.margin_y;
        }
        let avail = self.height - 2.0 * self.margin_y;
        self.margin_y + (avail - self.lines_per_page() as f32 * self.unit()) / 2.0
    }
}

#[derive(Clone, Debug)]
pub struct PlacedLine {
    pub x: f32,
    pub baseline: f32,
    pub runs: Vec<GlyphRun>,
    /// Блок документа, которому принадлежит строка (для касаний и правки).
    pub block: usize,
    /// Номер первого символа строки в блоке: вместе с блоком — позиция чтения.
    pub start: u32,
    /// Межсловный пробел относительно обычного (см. `para::Line::spacing`).
    pub spacing: Option<f32>,
    pub hyphen: bool,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub lines: Vec<PlacedLine>,
    /// Высота страницы; в режиме прокрутки — высота всей главы.
    pub height: f32,
    pub start: Position,
    /// Индекс главы в `Document::toc`.
    pub chapter: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Brk {
    Allow,
    /// Разрыв нежелателен (внутри строфы), но допустим.
    Cost,
    Never,
}

enum V {
    /// `h` — высота в единицах сетки, `base` — от верха строки до базовой линии, px.
    Line { h: f32, base: f32, line: Line, block: usize, brk: Brk },
    Space { units: f32, discard: bool, brk: Brk },
    Break,
}

const EPS: f32 = 1e-3;

pub fn layout(doc: &Document, fonts: &FontSet, hy: &Hyphenator, g: &Geometry) -> Vec<Page> {
    layout_segments(doc, &segments(doc), fonts, hy, g).into_iter().flatten().collect()
}

/// Куски книги между принудительными разрывами страницы (главы). Каждый верстается
/// независимо от остальных, поэтому после правки абзаца пересобирается только его глава.
pub fn segments(doc: &Document) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in doc.blocks.iter().enumerate() {
        if b.kind == Kind::PageBreak {
            if i > start {
                out.push(start..i);
            }
            start = i + 1;
        }
    }
    if doc.blocks.len() > start {
        out.push(start..doc.blocks.len());
    }
    out
}

/// Вёрстка нескольких кусков параллельно, по куску на свободное ядро.
pub fn layout_segments(
    doc: &Document,
    segs: &[Range<usize>],
    fonts: &FontSet,
    hy: &Hyphenator,
    g: &Geometry,
) -> Vec<Vec<Page>> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(segs.len().max(1));
    if threads <= 1 {
        return segs.iter().map(|r| layout_range(doc, r.clone(), fonts, hy, g)).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<Vec<Page>> = vec![Vec::new(); segs.len()];
    let done: Vec<Vec<(usize, Vec<Page>)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(r) = segs.get(i) else { break };
                        mine.push((i, layout_range(doc, r.clone(), fonts, hy, g)));
                    }
                    mine
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().expect("поток вёрстки")).collect()
    });
    for (i, pages) in done.into_iter().flatten() {
        out[i] = pages;
    }
    out
}

/// Вёрстка блоков `range` (обычно одной главы) с новой страницы.
pub fn layout_range(doc: &Document, range: Range<usize>, fonts: &FontSet, hy: &Hyphenator, g: &Geometry) -> Vec<Page> {
    let items = vertical_list(doc, range, fonts, hy, g);
    let pages = paginate(&items, g.lines_per_page() as f32);
    place(&items, &pages, doc, g)
}

/// Одна строка текста (для строки статуса и подписей).
pub fn text_line(
    text: &str,
    size: f32,
    align: Align,
    width: f32,
    fonts: &FontSet,
    hy: &Hyphenator,
    lang: Lang,
) -> Option<Line> {
    let mut st = ParaStyle::body(size);
    st.align = align;
    st.hyphenate = false;
    layout_paragraph(&[Span::plain(text)], lang, &st, width, fonts, hy).into_iter().next()
}

pub(crate) struct BlockStyle {
    pub(crate) para: ParaStyle,
    before: f32,
    after: f32,
}

pub(crate) fn block_style(b: &Block, g: &Geometry) -> BlockStyle {
    let fs = g.font_size;
    let w = g.text_width();
    let mut p = ParaStyle::body(fs);
    p.bionic = g.bionic && b.kind != Kind::Pre;
    p.indent = 1.3 * fs;
    let (mut before, mut after) = (0.0, 0.0);
    match b.kind {
        Kind::Heading(level) => {
            p.size = fs * match level {
                0 => 1.6,
                1 => 1.4,
                2 => 1.2,
                _ => 1.05,
            };
            p.align = Align::Center;
            p.indent = 0.0;
            p.hyphenate = false;
            p.bold = true;
            before = if level <= 2 { 2.0 } else { 1.0 };
            after = 1.0;
        }
        Kind::Subtitle => {
            p.align = Align::Center;
            p.indent = 0.0;
            p.hyphenate = false;
            before = 1.0;
            after = 1.0;
        }
        Kind::TextAuthor | Kind::Date => {
            p.align = Align::Right;
            p.indent = 0.0;
            p.hyphenate = false;
            p.italic = true;
        }
        Kind::PoemTitle => {
            p.align = Align::Left;
            p.indent = 0.0;
            p.bold = true;
            p.hyphenate = false;
        }
        Kind::Verse => {
            p.align = Align::Left;
            p.indent = 0.0;
            p.hyphenate = false;
        }
        Kind::TableRow => {
            p.align = Align::Left;
            p.indent = 0.0;
        }
        Kind::Pre => {
            p.align = Align::Left;
            p.indent = 0.0;
            p.hyphenate = false;
            p.size = fs * 0.85;
        }
        _ => {}
    }
    // CSS книги (EPUB): выравнивание, отступ первой строки, поля абзаца.
    let css = &b.css;
    if let Some(a) = css.align {
        p.align = a;
    }
    if let Some(i) = css.indent {
        p.indent = i.max(0.0) * fs;
    }
    let max_side = w * 0.45;
    p.left += (css.left_em * fs + css.left_pct * w).clamp(0.0, max_side);
    p.right += (css.right_em * fs + css.right_pct * w).clamp(0.0, max_side);
    if let Some(i) = css.indent.filter(|i| *i < 0.0) {
        // text-indent: -1em; margin-left: 1em — первая строка левее остальных.
        let hang = (-i * fs).min(p.left);
        p.left -= hang;
        p.hanging += hang;
    }
    if b.ctx.epigraph {
        // Стихам в эпиграфе нужна колонка пошире, чтобы строки не переносились.
        p.left += if b.ctx.poem { w * 0.2 } else { w * 0.3 };
        p.size *= 0.92;
        if b.kind == Kind::Para {
            p.align = Align::Left;
            p.indent = 0.0;
            p.italic = true;
        }
    }
    if b.ctx.cite {
        p.left += 1.5 * fs;
        p.right += 1.5 * fs;
        // Короткая строка цитаты без знака конца предложения — запись, подпись, формула:
        // её набирают флагом влево, без красной строки, растяжения и переносов.
        if b.kind == Kind::Para && is_display(b) {
            p.align = Align::Left;
            p.indent = 0.0;
            p.hyphenate = false;
        }
    }
    if b.ctx.poem && !b.ctx.epigraph {
        p.left += w * 0.08;
    }
    BlockStyle { para: p, before, after }
}

/// Абзац — не проза: до 100 знаков и не кончается знаком конца предложения
/// (точка, вопрос, восклицание, многоточие, двоеточие, запятая, точка с запятой,
/// в том числе перед закрывающей кавычкой). Знаки сносок не считаются.
fn is_display(b: &Block) -> bool {
    let text: String = b.spans.iter().filter(|s| s.note.is_none()).map(|s| s.text.as_str()).collect();
    let t: Vec<char> = text.trim_end().chars().collect();
    let Some(&last) = t.last() else { return false };
    let before_quote = if "»\"“”’)".contains(last) { t.iter().rev().nth(1).copied().unwrap_or(last) } else { last };
    t.len() <= 100 && !".!?…:;,".contains(before_quote)
}

fn vertical_list(doc: &Document, range: Range<usize>, fonts: &FontSet, hy: &Hyphenator, g: &Geometry) -> Vec<V> {
    let mut v: Vec<V> = Vec::new();
    let unit = g.unit();
    let tw = g.text_width();
    let mut prev_ctx: Option<Context> = None;
    let mut prev_heading = false;
    for (bi, b) in doc.blocks.iter().enumerate().skip(range.start).take(range.len()) {
        match b.kind {
            Kind::PageBreak => {
                v.push(V::Break);
                prev_ctx = None;
                continue;
            }
            Kind::EmptyLine | Kind::StanzaGap => {
                space(&mut v, 1.0);
                continue;
            }
            // Картинки — этап M2.
            Kind::Image(_) => continue,
            _ => {}
        }
        if prev_ctx.is_some_and(|pc| pc != b.ctx) {
            space(&mut v, 1.0);
        }
        prev_ctx = Some(b.ctx);

        let bs = block_style(b, g);
        let heading = matches!(b.kind, Kind::Heading(_));
        let at_top = matches!(v.last(), None | Some(V::Break));
        if heading && at_top {
            // Глава начинается с отступа сверху, как на Kindle.
            v.push(V::Space { units: 2.0, discard: false, brk: Brk::Never });
        } else if bs.before > 0.0 && !(heading && prev_heading) {
            space(&mut v, bs.before);
        }

        let lines = layout_paragraph(&b.spans, b.lang, &bs.para, tw, fonts, hy);
        // Мелкий кегль (эпиграф) тоже стоит на сетке: строка — ровно одна единица.
        let lh = (bs.para.size * g.leading).max(unit);
        let units = lh / unit;
        let n = lines.len();
        let total = units * n as f32;
        let pad = ((total - EPS).ceil() - total).max(0.0);
        let face = &fonts.faces[REGULAR];
        let (asc, desc) = (face.ascender * bs.para.size, face.descender * bs.para.size);
        let base_in = (lh - asc - desc) / 2.0 + asc;
        for (i, line) in lines.into_iter().enumerate() {
            let top_pad = if i == 0 { pad / 2.0 } else { 0.0 };
            let bottom_pad = if i + 1 == n { pad / 2.0 } else { 0.0 };
            v.push(V::Line {
                h: units + top_pad + bottom_pad,
                base: top_pad * unit + base_in,
                line,
                block: bi,
                brk: line_brk(doc, bi, b, i, n),
            });
        }
        let next_heading = matches!(doc.blocks.get(bi + 1).map(|x| &x.kind), Some(Kind::Heading(_)));
        if bs.after > 0.0 && !(heading && next_heading) {
            space(&mut v, bs.after);
        }
        prev_heading = heading;
    }
    v
}

/// Вертикальный пробел; подряд идущие пробелы не складываются, берётся больший.
fn space(v: &mut Vec<V>, units: f32) {
    if let Some(V::Space { units: u, discard: true, .. }) = v.last_mut() {
        *u = u.max(units);
        return;
    }
    let brk = match v.last() {
        Some(V::Line { brk, .. }) | Some(V::Space { brk, .. }) => *brk,
        _ => Brk::Never,
    };
    v.push(V::Space { units, discard: true, brk });
}

/// Можно ли разорвать страницу после строки `i` из `n` строк блока.
fn line_brk(doc: &Document, bi: usize, b: &Block, i: usize, n: usize) -> Brk {
    let kind_at = |k: usize| doc.blocks.get(k).map(|x| &x.kind);
    let next = kind_at(bi + 1);
    match b.kind {
        // Заголовок не остаётся внизу страницы.
        Kind::Heading(_) | Kind::Subtitle | Kind::PoemTitle => Brk::Never,
        Kind::Verse => {
            if i + 1 < n {
                return Brk::Never;
            }
            if !matches!(next, Some(Kind::Verse)) {
                return Brk::Allow;
            }
            // Строфа не оставляет одну строку внизу или вверху страницы.
            let first = bi == 0 || !matches!(kind_at(bi - 1), Some(Kind::Verse));
            let next_is_last = !matches!(kind_at(bi + 2), Some(Kind::Verse));
            if first || next_is_last {
                Brk::Never
            } else {
                Brk::Cost
            }
        }
        _ => {
            if i + 1 < n {
                // Висячие строки: минимум две строки абзаца внизу и вверху страницы.
                if i + 1 >= 2 && n - i - 1 >= 2 {
                    Brk::Allow
                } else {
                    Brk::Never
                }
            } else if matches!(next, Some(Kind::TextAuthor)) {
                Brk::Never
            } else {
                Brk::Allow
            }
        }
    }
}

fn height(x: &V) -> f32 {
    match x {
        V::Line { h, .. } => *h,
        V::Space { units, .. } => *units,
        V::Break => 0.0,
    }
}

fn brk_after(x: &V) -> Brk {
    match x {
        V::Line { brk, .. } | V::Space { brk, .. } => *brk,
        V::Break => Brk::Allow,
    }
}

/// Раскладывает вертикальный список по страницам; возвращает индексы элементов каждой страницы.
fn paginate(v: &[V], cap: f32) -> Vec<Vec<usize>> {
    let mut pages = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut used = 0.0f32;
    let mut i = 0;
    while i < v.len() {
        let h = height(&v[i]);
        match &v[i] {
            V::Break => {
                if !cur.is_empty() {
                    pages.push(std::mem::take(&mut cur));
                    used = 0.0;
                }
                i += 1;
                continue;
            }
            V::Space { discard: true, .. } if cur.is_empty() => {
                i += 1;
                continue;
            }
            _ => {}
        }
        if used + h <= cap + EPS || cur.is_empty() {
            cur.push(i);
            used += h;
            i += 1;
            continue;
        }
        // Не влезает: ищем лучшую точку разрыва на уже собранной странице.
        let k = choose_break(v, &cur, cap);
        let next = if k < cur.len() { cur[k] } else { i };
        cur.truncate(k);
        pages.push(std::mem::take(&mut cur));
        used = 0.0;
        i = next;
    }
    if !cur.is_empty() {
        pages.push(cur);
    }
    pages
}

/// Сколько элементов оставить на странице. Предпочитаем последний разрешённый разрыв;
/// разрыв внутри строфы — если он заметно полнее; если страница выходит пустой
/// больше чем наполовину, правила нарушаются (как у Kindle при длинных блоках).
fn choose_break(v: &[V], cur: &[usize], cap: f32) -> usize {
    let mut fills = Vec::with_capacity(cur.len());
    let mut fill = 0.0;
    for &ix in cur {
        fill += height(&v[ix]);
        fills.push(fill);
    }
    let mut allow = None;
    let mut cost = None;
    for j in (1..=cur.len()).rev() {
        match brk_after(&v[cur[j - 1]]) {
            Brk::Allow => {
                allow.get_or_insert(j);
                cost.get_or_insert(j);
                break;
            }
            Brk::Cost => {
                cost.get_or_insert(j);
            }
            Brk::Never => {}
        }
    }
    let full = |j: usize| fills[j - 1] >= cap * 0.5;
    match (allow, cost) {
        (Some(a), Some(c)) if c > a && fills[a - 1] < cap - 3.0 && full(c) => c,
        (Some(a), _) if full(a) => a,
        (_, Some(c)) if full(c) => c,
        _ => cur.len(),
    }
}

fn place(v: &[V], pages: &[Vec<usize>], doc: &Document, g: &Geometry) -> Vec<Page> {
    let unit = g.unit();
    let top = g.text_top();
    let mut out = Vec::new();
    for ixs in pages {
        let mut y = top;
        let mut lines = Vec::new();
        let mut start = None;
        for &ix in ixs {
            match &v[ix] {
                V::Line { h, base, line, block, .. } => {
                    start.get_or_insert(Position { block: *block as u32, offset: line.start });
                    lines.push(PlacedLine {
                        x: g.margin_x + line.x,
                        baseline: y + base,
                        runs: line.runs.clone(),
                        block: *block,
                        start: line.start,
                        spacing: line.spacing,
                        hyphen: line.hyphen,
                    });
                    y += h * unit;
                }
                V::Space { units, .. } => y += units * unit,
                V::Break => {}
            }
        }
        let Some(start) = start else { continue };
        let chapter = doc.toc.iter().rposition(|t| t.block as u32 <= start.block);
        let height = if g.scroll { y + g.margin_y } else { g.height };
        out.push(Page { lines, height, start, chapter });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockCss, InlineStyle};

    fn para(parts: &[(&str, Option<&str>)]) -> Block {
        let spans = parts
            .iter()
            .map(|(t, n)| Span { text: t.to_string(), style: InlineStyle::default(), note: n.map(String::from) })
            .collect();
        Block { kind: Kind::Para, ctx: Context { cite: true, ..Default::default() }, spans, lang: Lang::Ru, css: BlockCss::default() }
    }

    #[test]
    fn display_lines_in_cite() {
        assert!(is_display(&para(&[("4/33 САМАЯ ЗАМЕЧАТЕЛЬНАЯ ФОРМУЛА В МАТЕМАТИКЕ", None)])));
        assert!(is_display(&para(&[("e", None), ("iπ", None), (" + 1 = 0", None)])));
        assert!(!is_display(&para(&[("Короткая фраза.", None)])));
        assert!(!is_display(&para(&[("Конец цитаты.", None), ("1", Some("n1"))])));
        assert!(!is_display(&para(&[("«Так и сказал?»", None)])));
        assert!(!is_display(&para(&[("Длинный абзац без точки в конце ".repeat(5).as_str(), None)])));
    }

    #[test]
    fn wrapped_verses_keep_stanza_left_edge() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fonts");
        let fonts = FontSet::from_prefix(&format!("{root}/PTSerif"), &format!("{root}/LiberationMono-Regular.ttf")).unwrap();
        let doc = crate::fb2::parse(
            "<FictionBook><body><poem><stanza>
             <v>Короткая строка.</v>
             <v>Дом снимут, пройдет день-другой,</v>
             <v>Одним — ликованье, другим — огорченье,</v>
             </stanza></poem></body></FictionBook>".as_bytes(),
        ).unwrap();
        let hy = Hyphenator::new();
        for scroll in [false, true] {
            for bionic in [false, true] {
                let mut g = Geometry::new(576.0, 1280.0, 62.0, fonts.faces[REGULAR].x_height, scroll);
                g.bionic = bionic;
                let pages = layout(&doc, &fonts, &hy, &g);
                let lines: Vec<_> = pages.iter().flat_map(|p| &p.lines).collect();
                let left = lines[0].x;
                for (i, block) in doc.blocks.iter().enumerate().filter(|(_, b)| b.kind == Kind::Verse) {
                    let verse: Vec<_> = lines.iter().filter(|l| l.block == i).collect();
                    if block.text().chars().count() > 20 {
                        assert!(verse.len() >= 2, "длинная строка должна перенестись: {}", block.text());
                        assert!(verse[1].start > 0);
                    }
                    assert!(verse.iter().all(|l| (l.x - left).abs() < EPS),
                        "все строки строфы должны начинаться от одного края: {}", block.text());
                }
            }
        }
    }
}
