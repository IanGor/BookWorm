//! Вёрстка абзаца: шейпинг, точки разрыва, алгоритм Кнута — Пласса, сборка строк.
//!
//! Абзац превращается в последовательность элементов TeX: box (неразрывный
//! кусок текста), glue (пробел, который может растягиваться и сжиматься) и
//! penalty (возможный разрыв со штрафом, например перенос). Кнут — Пласс
//! выбирает разрывы, минимизирующие «некрасивость» всего абзаца сразу.

use crate::font::{FontSet, Glyph, MONO};
use crate::hyph::{is_combining, script_lang, Hyphenator};
use crate::model::{Lang, Span};
use crate::typo::{NBSP, NNBSP, SHY, ZWSP};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Justify,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct ParaStyle {
    /// Кегль в пикселях.
    pub size: f32,
    pub align: Align,
    /// Отступ первой строки.
    pub indent: f32,
    /// Поля абзаца внутри текстовой области.
    pub left: f32,
    pub right: f32,
    /// Дополнительный отступ строк после первой (продолжение строки стиха).
    pub hanging: f32,
    pub hyphenate: bool,
    pub italic: bool,
    pub bold: bool,
}

impl ParaStyle {
    pub fn body(size: f32) -> ParaStyle {
        ParaStyle {
            size,
            align: Align::Justify,
            indent: 0.0,
            left: 0.0,
            right: 0.0,
            hanging: 0.0,
            hyphenate: true,
            italic: false,
            bold: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

/// Глифы одного начертания и кегля, координаты относительно начала строки и базовой линии.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    pub face: u8,
    pub size: f32,
    pub glyphs: Vec<PGlyph>,
    pub x0: f32,
    pub x1: f32,
    pub strike: bool,
    pub note: Option<String>,
    /// Горизонтальный масштаб глифов (микротипографика): 1 — без изменений.
    pub xscale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Line {
    pub runs: Vec<GlyphRun>,
    /// Сдвиг строки от левого края текстовой области.
    pub x: f32,
    pub width: f32,
    /// Номер первого символа строки в абзаце.
    pub start: u32,
    /// Межсловный пробел строки относительно обычного (1 — как в шрифте);
    /// `None` — строка не выключалась (последняя, не по ширине, без пробелов).
    pub spacing: Option<f32>,
    /// Строка кончается переносом.
    pub hyphen: bool,
}

// Параметры качества. Подбираются по эталонным страницам.
const INF: f32 = 10_000.0;
const LINE_PENALTY: f64 = 10.0;
const HYPHEN_COST: f32 = 50.0;
const EXPLICIT_COST: f32 = 50.0;
const URL_COST: f32 = 100.0;
const DOUBLE_HYPHEN: f64 = 3000.0;
const TRIPLE_HYPHEN: f64 = 50_000.0;
const FINAL_HYPHEN: f64 = 5000.0;
const FITNESS: f64 = 100.0;
const SHORT_LAST: f64 = 2000.0;
/// Растяжение и сжатие межсловного пробела в долях его ширины: как в книжном наборе,
/// пробел остаётся в пределах 0,8–1,25 обычного (SPEC §2.5).
const SPACE_STRETCH: f32 = 0.25;
const SPACE_SHRINK: f32 = 0.2;
/// Микротипографика: остаток недостачи ширины строки делится по всем буквам, а не по
/// трём-четырём пробелам. Масштаб глифа по ширине — до ±3 %, межбуквенный интервал —
/// до +2 % / −1 % кегля. Так выключают pdfTeX (microtype) и InDesign; глазом не видно.
/// Подобрано по `examples/spacing.rs` на настоящей прозе (tests/corpus/real).
const EXPAND: f32 = 0.03;
const TRACK_STRETCH: f32 = 0.02;
const TRACK_SHRINK: f32 = 0.01;
/// Ширина узкого неразрывного пробела в долях обычного.
const NARROW_SPACE: f32 = 0.6;
/// Подъём верхнего и опускание нижнего индекса в долях кегля.
const SUP_RISE: f32 = 0.36;
const SUB_DROP: f32 = 0.16;
const SCRIPT_SCALE: f32 = 0.65;

const NO_RUN: usize = usize::MAX;

struct Run {
    face: usize,
    size: f32,
    dy: f32,
    strike: bool,
    note: Option<String>,
    glyphs: Vec<Glyph>,
    hyphen: Vec<Glyph>,
}

#[derive(Clone, Copy, Debug)]
enum Item {
    /// `st`, `sh` — упругость букв бокса (микротипографика).
    Box { w: f32, st: f32, sh: f32, run: usize, g0: usize, g1: usize, c0: u32 },
    Glue { w: f32, st: f32, sh: f32, fil: bool, brk: bool },
    /// `run` — чьим дефисом рисовать перенос (если `w > 0`).
    Penalty { w: f32, cost: f32, flagged: bool, run: usize },
}

#[derive(Clone, Copy)]
struct Brk {
    cost: f32,
    flagged: bool,
    hyphen: bool,
}

/// Верстает абзац в строки ширины `width` (ширина текстовой области).
pub fn layout_paragraph(
    spans: &[Span],
    lang: Lang,
    st: &ParaStyle,
    width: f32,
    fonts: &FontSet,
    hy: &Hyphenator,
) -> Vec<Line> {
    let mut p = Para {
        st,
        width,
        fonts,
        runs: Vec::new(),
        items: Vec::new(),
        rag: 0.0,
        sw: Vec::new(),
        sst: Vec::new(),
        ssh: Vec::new(),
        sfil: Vec::new(),
        sgst: Vec::new(),
    };
    p.build(spans, lang, hy);
    if !p.items.iter().any(|i| matches!(i, Item::Box { run, .. } if *run != NO_RUN)) {
        return Vec::new();
    }
    p.sums();
    let breaks = p
        .knuth_plass(1.0, 0.0)
        .or_else(|| p.knuth_plass(2.5, 0.0))
        .or_else(|| p.knuth_plass(10.0, 0.15 * width))
        .unwrap_or_else(|| p.greedy());
    p.lines(&breaks)
}

struct Para<'a> {
    st: &'a ParaStyle,
    width: f32,
    fonts: &'a FontSet,
    runs: Vec<Run>,
    items: Vec<Item>,
    /// Воображаемое растяжение строки при выключке не по ширине (приём TeX для ragged right).
    rag: f32,
    // Префиксные суммы по items: ширина, растяжение, сжатие, бесконечные растяжения.
    sw: Vec<f32>,
    sst: Vec<f32>,
    ssh: Vec<f32>,
    sfil: Vec<f32>,
    /// Растяжение одних пробелов (без букв): по нему считается некрасивость строки.
    sgst: Vec<f32>,
}

#[derive(Clone, Copy)]
struct Node {
    pos: usize,
    /// Индекс элемента, с которого начинается следующая строка.
    start: usize,
    line: usize,
    fit: u8,
    hy: u8,
    flagged: bool,
    dem: f64,
    prev: usize,
}

impl Para<'_> {
    fn build(&mut self, spans: &[Span], lang: Lang, hy: &Hyphenator) {
        let st = self.st;
        let mut chars: Vec<char> = Vec::new();
        let mut owner: Vec<usize> = Vec::new();
        for (si, s) in spans.iter().enumerate() {
            for c in s.text.chars() {
                chars.push(c);
                owner.push(si);
            }
        }
        let n = chars.len();
        let mut brk: Vec<Option<Brk>> = vec![None; n + 1];
        self.mark_breaks(&chars, lang, hy, &mut brk);

        // Начертание, кегль и сдвиг базовой линии каждого фрагмента.
        let fmt: Vec<(usize, f32, f32)> = spans
            .iter()
            .map(|s| {
                let mut style = s.style;
                style.italic ^= st.italic;
                style.bold |= st.bold;
                let face = FontSet::face_index(&style);
                let script = s.style.sup || s.style.sub;
                let size = if script { st.size * SCRIPT_SCALE } else { st.size };
                let dy = if s.style.sup {
                    -SUP_RISE * st.size
                } else if s.style.sub {
                    SUB_DROP * st.size
                } else {
                    0.0
                };
                (face, size, dy)
            })
            .collect();

        self.rag = if st.align == Align::Justify { 0.0 } else { 3.0 * st.size };
        if st.indent > 0.0 {
            self.items.push(Item::Box { w: st.indent, st: 0.0, sh: 0.0, run: NO_RUN, g0: 0, g1: 0, c0: 0 });
        }

        let mut i = 0;
        while i < n {
            let c = chars[i];
            match c {
                ' ' | NBSP => {
                    let (face, size, _) = fmt[owner[i]];
                    // Пробел верхнего индекса не должен быть узким: берём основной кегль.
                    let w = self.fonts.metrics(face, size.max(st.size)).space;
                    let (stv, shv) =
                        if st.align == Align::Justify { (w * SPACE_STRETCH, w * SPACE_SHRINK) } else { (0.0, 0.0) };
                    self.items.push(Item::Glue { w, st: stv, sh: shv, fil: false, brk: c == ' ' });
                    i += 1;
                }
                NNBSP => {
                    let (face, size, _) = fmt[owner[i]];
                    let w = self.fonts.metrics(face, size).space * NARROW_SPACE;
                    self.items.push(Item::Glue { w, st: 0.0, sh: 0.0, fil: false, brk: false });
                    i += 1;
                }
                ZWSP => {
                    self.items.push(Item::Penalty { w: 0.0, cost: 0.0, flagged: false, run: NO_RUN });
                    i += 1;
                }
                // Точка переноса уже отмечена в brk для следующего символа.
                SHY => i += 1,
                _ => {
                    let si = owner[i];
                    let mut j = i;
                    while j < n && owner[j] == si && !matches!(chars[j], ' ' | NBSP | NNBSP | ZWSP | SHY) {
                        j += 1;
                    }
                    self.piece(&chars[i..j], i, &spans[si], fmt[si], lang, &brk);
                    i = j;
                }
            }
        }
        self.items.push(Item::Penalty { w: 0.0, cost: INF, flagged: false, run: NO_RUN });
        self.items.push(Item::Glue { w: 0.0, st: 0.0, sh: 0.0, fil: true, brk: false });
        self.items.push(Item::Penalty { w: 0.0, cost: -INF, flagged: false, run: NO_RUN });
    }

    /// Отмечает допустимые разрывы внутри слов: переносы по словарю, мягкие переносы,
    /// дефисы составных слов, тире без пробелов, адреса.
    fn mark_breaks(&self, chars: &[char], lang: Lang, hy: &Hyphenator, brk: &mut [Option<Brk>]) {
        let n = chars.len();
        let hyphen = Brk { cost: HYPHEN_COST, flagged: true, hyphen: true };
        // Последнее слово абзаца не переносится: иначе последняя строка — огрызок слова.
        let last_word = chars.iter().rposition(|c| c.is_alphabetic()).map_or(0, |e| {
            chars[..e].iter().rposition(|c| !is_letter(*c)).map_or(0, |s| s + 1)
        });
        let mut t = 0;
        while t < n {
            if matches!(chars[t], ' ' | NBSP) {
                t += 1;
                continue;
            }
            let mut e = t;
            while e < n && !matches!(chars[e], ' ' | NBSP) {
                e += 1;
            }
            let tok = &chars[t..e];
            let url = is_url(tok);
            for k in t..e {
                let c = chars[k];
                let prev = if k > t { chars[k - 1] } else { '\0' };
                let next = if k + 1 < e { chars[k + 1] } else { '\0' };
                if c == SHY && k + 1 < e {
                    brk[k + 1] = Some(hyphen);
                }
                if url {
                    if k + 1 < e && "/._\\-?&=#".contains(c) {
                        brk[k + 1] = Some(Brk { cost: URL_COST, flagged: false, hyphen: false });
                    }
                    continue;
                }
                if c == '-' && prev.is_alphabetic() && next.is_alphabetic() {
                    brk[k + 1] = Some(Brk { cost: EXPLICIT_COST, flagged: true, hyphen: false });
                }
                if (c == '—' || c == '–') && next != '\0' {
                    let cost = if c == '—' { 0.0 } else { URL_COST };
                    brk[k + 1] = Some(Brk { cost, flagged: false, hyphen: false });
                }
            }
            if !url && self.st.hyphenate && !tok.contains(&SHY) {
                let mut w = t;
                while w < e {
                    if !is_letter(chars[w]) {
                        w += 1;
                        continue;
                    }
                    let mut z = w;
                    while z < e && is_letter(chars[z]) {
                        z += 1;
                    }
                    // Составные слова переносятся только по дефису.
                    let compound = (w > t && chars[w - 1] == '-') || (z < e && chars[z] == '-');
                    if !compound && w < last_word {
                        if let Some(l) = script_lang(&chars[w..z], lang) {
                            for p in hy.points(&chars[w..z], l) {
                                brk[w + p] = Some(hyphen);
                            }
                        }
                    }
                    w = z;
                }
            }
            t = e;
        }
    }

    /// Кусок текста одного фрагмента без пробелов: шейпится целиком (кернинг и лигатуры
    /// сохраняются), а по отмеченным точкам разрыва режется на боксы.
    fn piece(&mut self, chars: &[char], start: usize, span: &Span, fmt: (usize, f32, f32), lang: Lang, brk: &[Option<Brk>]) {
        let (face, size, dy) = fmt;
        let text: String = chars.iter().collect();
        let glyphs = self.fonts.faces[face].shape(&text, size, lang);
        let hyphen = self.fonts.metrics(face, size).hyphen;

        let mut offs = Vec::with_capacity(chars.len());
        let mut b = 0;
        for c in chars {
            offs.push(b);
            b += c.len_utf8();
        }
        // Разрез возможен только на границе кластера: внутри лигатуры не режем.
        let mut cuts: Vec<(usize, usize, Brk)> = Vec::new();
        for (k, &off) in offs.iter().enumerate().skip(1) {
            if let Some(bk) = brk[start + k] {
                if let Some(g) = glyphs.iter().position(|gl| gl.cluster >= off) {
                    if glyphs[g].cluster == off && g > 0 {
                        cuts.push((g, k, bk));
                    }
                }
            }
        }
        let total = glyphs.len();
        let run = self.runs.len();
        self.runs.push(Run { face, size, dy, strike: span.style.strike, note: span.note.clone(), glyphs, hyphen });

        if start > 0 {
            if let Some(bk) = brk[start] {
                if matches!(self.items.last(), Some(Item::Box { .. })) {
                    self.push_penalty(bk);
                }
            }
        }
        let (mut g0, mut c0) = (0, start);
        for (g, k, bk) in cuts {
            self.push_box(run, g0, g, c0);
            self.push_penalty(bk);
            g0 = g;
            c0 = start + k;
        }
        self.push_box(run, g0, total, c0);
    }

    fn push_box(&mut self, run: usize, g0: usize, g1: usize, c0: usize) {
        if g1 <= g0 {
            return;
        }
        let r = &self.runs[run];
        let w: f32 = r.glyphs[g0..g1].iter().map(|g| g.advance).sum();
        // Код не растягиваем: моноширинный текст держит колонки.
        let elastic = self.st.align == Align::Justify && r.face != MONO;
        let n = (g1 - g0) as f32;
        let (st, sh) = if elastic {
            (w * EXPAND + n * TRACK_STRETCH * r.size, w * EXPAND + n * TRACK_SHRINK * r.size)
        } else {
            (0.0, 0.0)
        };
        self.items.push(Item::Box { w, st, sh, run, g0, g1, c0: c0 as u32 });
    }

    fn push_penalty(&mut self, bk: Brk) {
        let run = self
            .items
            .iter()
            .rev()
            .find_map(|i| match i {
                Item::Box { run, .. } if *run != NO_RUN => Some(*run),
                _ => None,
            })
            .unwrap_or(NO_RUN);
        let w = if bk.hyphen && run != NO_RUN { self.runs[run].hyphen.iter().map(|g| g.advance).sum() } else { 0.0 };
        self.items.push(Item::Penalty { w, cost: bk.cost, flagged: bk.flagged, run });
    }

    fn sums(&mut self) {
        let n = self.items.len();
        self.sw = vec![0.0; n + 1];
        self.sst = vec![0.0; n + 1];
        self.ssh = vec![0.0; n + 1];
        self.sfil = vec![0.0; n + 1];
        self.sgst = vec![0.0; n + 1];
        for (i, it) in self.items.iter().enumerate() {
            let (w, st, sh, fil) = match *it {
                Item::Box { w, st, sh, .. } => (w, st, sh, 0.0),
                Item::Glue { w, st, sh, fil, .. } => (w, st, sh, if fil { 1.0 } else { 0.0 }),
                Item::Penalty { .. } => (0.0, 0.0, 0.0, 0.0),
            };
            self.sw[i + 1] = self.sw[i] + w;
            self.sst[i + 1] = self.sst[i] + st;
            self.ssh[i + 1] = self.ssh[i] + sh;
            self.sfil[i + 1] = self.sfil[i] + fil;
            let gst = if let Item::Glue { st, fil: false, .. } = *it { st } else { 0.0 };
            self.sgst[i + 1] = self.sgst[i] + gst;
        }
    }

    /// Начало следующей строки после разрыва в `b`: пробелы и штрафы отбрасываются.
    fn after(&self, b: usize) -> usize {
        let mut j = b;
        while j < self.items.len() {
            match self.items[j] {
                Item::Box { .. } => break,
                Item::Penalty { cost, .. } if cost <= -INF && j > b => break,
                _ => j += 1,
            }
        }
        j
    }

    fn line_width(&self, line: usize) -> f32 {
        let hanging = if line > 0 { self.st.hanging } else { 0.0 };
        self.width - self.st.left - self.st.right - hanging
    }

    /// Ширина разрыва в `b` и его штраф, если в `b` вообще можно разорвать строку.
    fn breakpoint(&self, b: usize) -> Option<(f32, f32, bool)> {
        match self.items[b] {
            Item::Penalty { w, cost, flagged, .. } if cost < INF => Some((w, cost, flagged)),
            Item::Glue { brk: true, .. } if b > 0 && matches!(self.items[b - 1], Item::Box { .. }) => {
                Some((0.0, 0.0, false))
            }
            _ => None,
        }
    }

    fn ratio(&self, s: usize, b: usize, pw: f32, lw: f32, emergency: f32) -> f32 {
        let nat = self.sw[b] - self.sw[s] + pw;
        // Бесконечное растяжение (конец абзаца) спасает от разреженности, но не от переполнения.
        if nat <= lw && self.sfil[b] - self.sfil[s] > 0.0 {
            0.0
        } else if nat < lw {
            let y = self.sst[b] - self.sst[s] + self.rag + emergency;
            if y > 0.0 {
                (lw - nat) / y
            } else {
                INF
            }
        } else if nat > lw {
            let z = self.ssh[b] - self.ssh[s];
            if z > 0.0 {
                (lw - nat) / z
            } else {
                -INF
            }
        } else {
            0.0
        }
    }

    /// Насколько растянуты именно пробелы строки (в долях их растяжения): буквы
    /// забирают свою долю недостачи до предела, остальное достаётся пробелам —
    /// так же, как при сборке строки в `lines`. Некрасивость считается по этому числу.
    fn space_ratio(&self, s: usize, b: usize, pw: f32, lw: f32) -> f32 {
        let r = self.ratio(s, b, pw, lw, 0.0);
        if self.st.align != Align::Justify || r <= 1.0 {
            return r;
        }
        let gst = self.sgst[b] - self.sgst[s];
        let lst = self.sst[b] - self.sst[s] - gst;
        let slack = lw - (self.sw[b] - self.sw[s] + pw);
        if gst > 0.0 {
            (slack - lst) / gst
        } else {
            INF
        }
    }

    /// Кнут — Пласс. `tol` — допустимый коэффициент растяжения строки,
    /// `emergency` — аварийное растяжение для абзацев, которые иначе не собрать.
    fn knuth_plass(&self, tol: f32, emergency: f32) -> Option<Vec<usize>> {
        let mut nodes =
            vec![Node { pos: 0, start: 0, line: 0, fit: 1, hy: 0, flagged: false, dem: 0.0, prev: usize::MAX }];
        let mut active: Vec<usize> = vec![0];
        let last = self.items.len() - 1;
        for b in 0..self.items.len() {
            let Some((pw, cost, flagged)) = self.breakpoint(b) else { continue };
            let forced = cost <= -INF;
            let mut best: Vec<(f64, usize, u8, u8)> = Vec::new();
            let mut keep = Vec::with_capacity(active.len());
            for &ai in &active {
                let a = nodes[ai];
                let lw = self.line_width(a.line);
                let r = self.ratio(a.start, b, pw, lw, emergency);
                if r >= -1.0 && !forced {
                    keep.push(ai);
                }
                if !(-1.0..=tol).contains(&r) {
                    continue;
                }
                // Аварийное растяжение только делает строку допустимой; некрасивость считаем
                // по настоящей разреженности, иначе алгоритм не отличит плохую строку от хорошей.
                let real = self.space_ratio(a.start, b, pw, lw).min(20.0);
                let hy = if flagged { a.hy.saturating_add(1).min(3) } else { 0 };
                let bad = 100.0 * (real.abs() as f64).powi(3);
                let mut d = (LINE_PENALTY + bad).powi(2);
                // Третий перенос подряд — почти запрет, но лучше, чем разреженный абзац.
                if hy > 2 {
                    d += TRIPLE_HYPHEN;
                }
                if cost >= 0.0 {
                    d += (cost as f64).powi(2);
                } else if !forced {
                    d -= (cost as f64).powi(2);
                }
                if flagged && a.flagged {
                    d += DOUBLE_HYPHEN;
                }
                let fit = if real < -0.5 {
                    0
                } else if real <= 0.5 {
                    1
                } else if real <= 1.0 {
                    2
                } else {
                    3
                };
                if (fit as i32 - a.fit as i32).abs() > 1 {
                    d += FITNESS;
                }
                if forced {
                    if a.flagged {
                        d += FINAL_HYPHEN;
                    }
                    let nat = self.sw[b] - self.sw[a.start];
                    if a.line > 0 && nat < 0.15 * lw {
                        d += SHORT_LAST;
                    }
                }
                let total = a.dem + d;
                match best.iter_mut().find(|c| c.2 == fit && c.3 == hy) {
                    Some(c) if total < c.0 => *c = (total, ai, fit, hy),
                    Some(_) => {}
                    None => best.push((total, ai, fit, hy)),
                }
            }
            active = keep;
            let start = self.after(b);
            for (dem, ai, fit, hy) in best {
                nodes.push(Node { pos: b, start, line: nodes[ai].line + 1, fit, hy, flagged, dem, prev: ai });
                active.push(nodes.len() - 1);
            }
            if active.is_empty() {
                return None;
            }
            if forced && b == last {
                break;
            }
        }
        let end = active
            .iter()
            .copied()
            .filter(|&i| nodes[i].pos == last)
            .min_by(|&x, &y| nodes[x].dem.total_cmp(&nodes[y].dem))?;
        let mut out = Vec::new();
        let mut i = end;
        while i != 0 {
            out.push(nodes[i].pos);
            i = nodes[i].prev;
        }
        out.reverse();
        Some(out)
    }

    /// Запасной вариант: жадное заполнение строк, допускает переполнение.
    fn greedy(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut start = 0;
        let mut fit: Option<usize> = None;
        for b in 0..self.items.len() {
            let Some((pw, cost, _)) = self.breakpoint(b) else { continue };
            let forced = cost <= -INF;
            let lw = self.line_width(out.len());
            if self.sw[b] - self.sw[start] + pw > lw {
                let cut = fit.unwrap_or(b);
                out.push(cut);
                start = self.after(cut);
                fit = None;
            }
            if forced {
                if out.last() != Some(&b) {
                    out.push(b);
                }
                break;
            }
            if out.last() != Some(&b) {
                fit = Some(b);
            }
        }
        out
    }

    fn lines(&self, breaks: &[usize]) -> Vec<Line> {
        let mut out = Vec::new();
        let mut s = 0;
        for (li, &b) in breaks.iter().enumerate() {
            while li > 0 && s < b && !matches!(self.items[s], Item::Box { .. }) {
                s += 1;
            }
            let lw = self.line_width(li);
            let last = li + 1 == breaks.len();
            let (pw, hrun) = match self.items[b] {
                Item::Penalty { w, run, .. } if w > 0.0 => (w, run),
                _ => (0.0, NO_RUN),
            };
            let nat = self.sw[b] - self.sw[s] + pw;
            // Упругость строки: пробелы и буквы отдельно.
            let (mut gst, mut gsh, mut glue_w) = (0.0f32, 0.0f32, 0.0f32);
            for it in &self.items[s..b] {
                if let Item::Glue { w, st, sh, fil: false, .. } = *it {
                    gst += st;
                    gsh += sh;
                    if st > 0.0 {
                        glue_w += w;
                    }
                }
            }
            let (lst, lsh) = (self.sst[b] - self.sst[s] - gst, self.ssh[b] - self.ssh[s] - gsh);
            let slack = lw - nat;
            // Последняя строка ставится как есть, но если не влезает — сжимается, как и остальные.
            let justified = self.st.align == Align::Justify && (!last || slack < 0.0);
            // Сначала пропорционально всем (r в [-1, 1]); что не влезло — пробелам.
            let (lr, gr) = if !justified {
                (0.0, 0.0)
            } else if slack >= 0.0 {
                let r = if lst + gst > 0.0 { slack / (lst + gst) } else { 0.0 };
                if r <= 1.0 || gst <= 0.0 {
                    (r.min(1.0), r)
                } else {
                    (1.0, (slack - lst) / gst)
                }
            } else {
                let r = if lsh + gsh > 0.0 { slack / (lsh + gsh) } else { 0.0 };
                (r.max(-1.0), r.max(-1.0))
            };
            // Строку из двух-трёх слов, которую пришлось бы растянуть дырой, ставим влево:
            // неровный край у одной строки лучше пробела в полстроки.
            let spaces = self.items[s..b].iter().filter(|i| matches!(i, Item::Glue { st, fil: false, .. } if *st > 0.0)).count();
            let (lr, gr) = if justified && spaces <= 2 && gr * SPACE_STRETCH > 1.0 { (0.0, 0.0) } else { (lr, gr) };
            let xscale = 1.0 + lr * EXPAND;
            let track = if lr >= 0.0 { lr * TRACK_STRETCH } else { lr * TRACK_SHRINK };

            let mut line = Line::default();
            if justified && glue_w > 0.0 {
                line.spacing = Some(1.0 + if gr >= 0.0 { gr * SPACE_STRETCH } else { gr * SPACE_SHRINK });
            }
            line.hyphen = hrun != NO_RUN;
            let mut x = 0.0f32;
            let mut cur = NO_RUN;
            let mut started = false;
            for k in s..b {
                match self.items[k] {
                    Item::Box { w, run, g0, g1, c0, .. } => {
                        if !started {
                            line.start = c0;
                            started = true;
                        }
                        if run == NO_RUN {
                            x += w;
                            continue;
                        }
                        if cur != run {
                            line.runs.push(self.glyph_run(run, x, xscale));
                            cur = run;
                        }
                        let (sc, tr) = if self.runs[run].face == MONO { (1.0, 0.0) } else { (xscale, track) };
                        x = self.emit(&mut line, run, &self.runs[run].glyphs[g0..g1], x, sc, tr);
                    }
                    Item::Glue { w, st, sh, fil, .. } => {
                        if !fil {
                            x += w + if gr >= 0.0 { gr * st } else { gr * sh };
                        }
                        cur = NO_RUN;
                    }
                    Item::Penalty { .. } => {}
                }
            }
            if hrun != NO_RUN {
                if cur != hrun {
                    line.runs.push(self.glyph_run(hrun, x, xscale));
                }
                x = self.emit(&mut line, hrun, &self.runs[hrun].hyphen, x, xscale, 0.0);
            }
            line.width = x;
            let off = match self.st.align {
                Align::Center => (lw - x) / 2.0,
                Align::Right => lw - x,
                _ => 0.0,
            };
            let hanging = if li > 0 { self.st.hanging } else { 0.0 };
            line.x = self.st.left + hanging + off.max(0.0);
            out.push(line);
            s = b;
        }
        out
    }

    fn glyph_run(&self, run: usize, x: f32, xscale: f32) -> GlyphRun {
        let r = &self.runs[run];
        GlyphRun {
            face: r.face as u8,
            size: r.size,
            glyphs: Vec::new(),
            x0: x,
            x1: x,
            strike: r.strike,
            note: r.note.clone(),
            xscale: if r.face == MONO { 1.0 } else { xscale },
        }
    }

    /// Глифы в строку: `scale` — масштаб по ширине, `track` — добавка после глифа в долях кегля.
    fn emit(&self, line: &mut Line, run: usize, glyphs: &[Glyph], mut x: f32, scale: f32, track: f32) -> f32 {
        let r = &self.runs[run];
        let (dy, add) = (r.dy, track * r.size);
        if let Some(gr) = line.runs.last_mut() {
            for g in glyphs {
                gr.glyphs.push(PGlyph { id: g.id, x: x + g.dx * scale, y: dy + g.dy });
                x += g.advance * scale + add;
            }
            gr.x1 = x;
        }
        x
    }
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic() || is_combining(c)
}

fn is_url(tok: &[char]) -> bool {
    let s: String = tok.iter().collect();
    s.contains("://") || s.contains("www.") || (s.contains('@') && s.contains('.')) || s.contains(":\\")
}
