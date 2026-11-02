//! Открытая книга: документ и свёрстанные страницы.
//! Через этот тип с ядром работают и экран чтения на телефоне, и bw-render.

use crate::fb2;
use crate::font::FontSet;
use crate::hyph::Hyphenator;
use crate::layout::{self, Geometry, Page, PlacedLine};
use crate::model::{Document, Position};
use crate::para::layout_paragraph;
use crate::zip;

/// Разбор файла книги: FB2, FB2 в zip или EPUB.
pub fn parse(bytes: &[u8]) -> Result<Document, String> {
    if !zip::is_zip(bytes) {
        return fb2::parse(bytes);
    }
    let entries = zip::entries(bytes)?;
    if crate::epub::is_epub(&entries) {
        return crate::epub::parse(bytes, &entries);
    }
    let e = entries
        .iter()
        .find(|e| e.name.to_lowercase().ends_with(".fb2"))
        .ok_or("в архиве нет ни FB2, ни EPUB")?;
    fb2::parse(&zip::read(bytes, e)?)
}

/// Что под пальцем.
#[derive(Debug, Default, PartialEq)]
pub struct Hit {
    pub block: Option<usize>,
    pub note: Option<String>,
}

pub struct Book {
    pub doc: Document,
    pub fonts: FontSet,
    pub hy: Hyphenator,
    pub g: Geometry,
    seg_pages: Vec<Vec<Page>>,
    /// Номер первой страницы каждого куска.
    seg_first: Vec<usize>,
}

impl Book {
    pub fn open(bytes: &[u8], fonts: FontSet, g: Geometry) -> Result<Book, String> {
        let doc = parse(bytes)?;
        let segs = layout::segments(&doc);
        let hy = Hyphenator::new();
        let seg_pages = layout::layout_segments(&doc, &segs, &fonts, &hy, &g);
        let mut book = Book {
            doc,
            fonts,
            hy,
            g,
            seg_pages,
            seg_first: Vec::new(),
        };
        book.reindex();
        Ok(book)
    }

    fn reindex(&mut self) {
        let mut n = 0;
        self.seg_first = self
            .seg_pages
            .iter()
            .map(|p| {
                let first = n;
                n += p.len();
                first
            })
            .collect();
    }

    pub fn page_count(&self) -> usize {
        self.seg_pages.iter().map(Vec::len).sum()
    }

    pub fn page(&self, n: usize) -> Option<&Page> {
        let s = self.seg_first.partition_point(|&f| f <= n).checked_sub(1)?;
        // Пустые куски дают одинаковые seg_first; берём последний с этим номером.
        self.seg_pages[s].get(n - self.seg_first[s])
    }

    /// Страница, на которой стоит позиция чтения.
    pub fn page_of(&self, pos: Position) -> usize {
        let total = self.page_count();
        let (mut lo, mut hi) = (0, total);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.page(mid).is_some_and(|p| p.start <= pos) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.saturating_sub(1).min(total.saturating_sub(1))
    }

    /// Прокрутка: позиция чтения для окна, верх которого на высоте `top` главы `n`
    /// (первая строка, целиком видимая под верхним полем).
    pub fn pos_at(&self, n: usize, top: f32) -> Position {
        let Some(page) = self.page(n) else { return Position::default() };
        let unit = self.g.unit();
        page.lines
            .iter()
            .find(|l| l.baseline - 0.8 * unit >= top + self.g.margin_y * 0.5)
            .or(page.lines.last())
            .map_or(page.start, |l| Position { block: l.block as u32, offset: l.start })
    }

    /// Прокрутка: где должен быть верх окна, чтобы позиция `pos` стояла под верхним полем.
    pub fn top_of(&self, n: usize, pos: Position) -> f32 {
        let Some(page) = self.page(n) else { return 0.0 };
        let unit = self.g.unit();
        let line = page.lines.iter().rev().find(|l| Position { block: l.block as u32, offset: l.start } <= pos);
        line.map_or(0.0, |l| (l.baseline - 0.8 * unit - self.g.margin_y).max(0.0))
    }

    /// Прокрутка: строка статуса для окна с верхом на высоте `top` главы `n`.
    /// Время — по оставшейся высоте главы, процент — по высоте всей книги.
    pub fn scroll_footer(&self, n: usize, top: f32) -> (String, String) {
        let screen = (self.g.height - 2.0 * self.g.margin_y).max(1.0);
        let heights: Vec<f32> = (0..self.page_count()).filter_map(|i| self.page(i).map(|p| p.height)).collect();
        let total: f32 = heights.iter().sum::<f32>().max(1.0);
        let here = heights.get(n).copied().unwrap_or(0.0);
        let rest = (here - top - self.g.height).max(0.0);
        let minutes = (rest / screen * 1.3).ceil() as u32;
        let done = heights[..n.min(heights.len())].iter().sum::<f32>() + top + self.g.height;
        let pct = ((done / total) * 100.0).clamp(0.0, 100.0) as u32;
        (format!("{minutes} мин до конца главы"), format!("{pct}%"))
    }

    /// Строка статуса: слева время до конца главы, справа процент книги.
    /// Скорость чтения пока условная (~1,3 мин на страницу), настоящая — по чтению.
    pub fn footer(&self, n: usize) -> (String, String) {
        let total = self.page_count().max(1);
        let chapter = self.page(n).and_then(|p| p.chapter);
        let rest = (n + 1..total).take_while(|&i| self.page(i).and_then(|p| p.chapter) == chapter).count() + 1;
        let minutes = (rest as f32 * 1.3).ceil() as u32;
        (format!("{minutes} мин до конца главы"), format!("{}%", (n + 1) * 100 / total))
    }

    /// Оглавление: (уровень, страница, название).
    pub fn toc(&self) -> Vec<(u8, usize, String)> {
        self.doc
            .toc
            .iter()
            .map(|t| (t.level, self.page_of(Position { block: t.block as u32, offset: 0 }), t.title.clone()))
            .collect()
    }

    /// Области касания знаков сносок на странице: (x0, y0, x1, y1) и id сноски.
    /// Знак маленький, поэтому область берётся с запасом.
    pub fn note_areas(&self, n: usize) -> Vec<([f32; 4], String)> {
        let Some(page) = self.page(n) else { return Vec::new() };
        let fs = self.g.font_size;
        let mut out = Vec::new();
        for l in &page.lines {
            for r in &l.runs {
                if let Some(id) = &r.note {
                    let area = [l.x + r.x0 - fs, l.baseline - 1.6 * fs, l.x + r.x1 + fs, l.baseline + 0.5 * fs];
                    out.push((area, id.clone()));
                }
            }
        }
        out
    }

    pub fn hit(&self, n: usize, x: f32, y: f32) -> Hit {
        let mut hit = Hit::default();
        let Some(page) = self.page(n) else { return hit };
        let centre = |a: &[f32; 4]| ((a[0] + a[2]) / 2.0 - x).abs() + ((a[1] + a[3]) / 2.0 - y).abs();
        hit.note = self
            .note_areas(n)
            .into_iter()
            .filter(|(a, _)| (a[0]..=a[2]).contains(&x) && (a[1]..=a[3]).contains(&y))
            .min_by(|a, b| centre(&a.0).total_cmp(&centre(&b.0)))
            .map(|(_, id)| id);
        let fs = self.g.font_size;
        let mut best = f32::MAX;
        for l in &page.lines {
            let (top, bottom) = (l.baseline - 0.9 * fs, l.baseline + 0.4 * fs);
            let d = (top - y).max(y - bottom).max(0.0);
            if d < best && d < self.g.unit() {
                best = d;
                hit.block = Some(l.block);
            }
        }
        hit
    }

    /// Selection rectangles in page/chapter coordinates, offsets in Android UTF-16 units.
    pub fn selection_cells(&self, n: usize, block: usize) -> String {
        let Some(page) = self.page(n) else { return String::new() };
        let Some(b) = self.doc.blocks.get(block) else { return String::new() };
        let text = b.text();
        let mut utf16 = vec![0];
        for c in text.chars() {
            utf16.push(utf16.last().unwrap() + c.len_utf16());
        }
        let mut out = String::new();
        for l in page.lines.iter().filter(|l| l.block == block) {
            for r in &l.runs {
                for g in &r.glyphs {
                    if let Some((a, b)) = g.source {
                        out.push_str(&format!("{} {} {} {} {} {}\n", utf16[a as usize], utf16[b as usize],
                            l.x + g.left, l.baseline - r.size * 0.9,
                            l.x + g.right, l.baseline + r.size * 0.3));
                    }
                }
            }
        }
        out
    }

    /// Контекст для воспроизведения ошибки без доступа к телефону.
    pub fn typography_report(&self, block: usize) -> Option<String> {
        let selected = self.doc.blocks.get(block)?;
        let mut out = format!("Книга: {}\nАвторы: {}\nБлок: {} (индексы с нуля)\nГеометрия: {:#?}\n",
            self.doc.title, self.doc.authors.join("; "), block, self.g);
        for i in block.saturating_sub(1)..(block + 2).min(self.doc.blocks.len()) {
            let b = &self.doc.blocks[i];
            out.push_str(&format!("\nБлок {i}, полный текст:\n{}\n\nСтруктура, стили и Unicode:\n{b:#?}\n", b.text()));
        }
        let chars: Vec<char> = selected.text().chars().collect();
        let lines: Vec<_> = (0..self.page_count()).flat_map(|n|
            self.page(n).into_iter().flat_map(move |p| p.lines.iter().filter(move |l| l.block == block).map(move |l| (n, l)))
        ).collect();
        out.push_str("\nФактические строки выбранного блока:\n");
        for (i, (page, l)) in lines.iter().enumerate() {
            let start = l.start as usize;
            let end = lines.get(i + 1).map_or(chars.len(), |(_, next)| next.start as usize);
            let text: String = chars.get(start..end).unwrap_or_default().iter().collect();
            out.push_str(&format!("страница/глава={}, offset={}, x={:.2}, baseline={:.2}, spacing={:?}, hyphen={}: {:?}\n",
                page + 1, start, l.x, l.baseline, l.spacing, l.hyphen, text));
            for r in &l.runs {
                out.push_str(&format!("  face={}, size={:.3}, x={:.3}..{:.3}, scale={:.5}\n", r.face, r.size, r.x0, r.x1, r.xscale));
            }
        }
        Some(out)
    }

    /// Текст сноски, свёрстанный тем же движком для попапа шириной во весь экран.
    /// Возвращает строки (координаты от верха попапа) и высоту попапа.
    pub fn note(&self, id: &str) -> Option<(Vec<PlacedLine>, f32)> {
        let blocks = self.doc.notes.get(id)?;
        let g = &self.g;
        let unit = g.unit();
        let face = &self.fonts.faces[crate::font::REGULAR];
        let base_in = (unit - (face.ascender + face.descender) * g.font_size) / 2.0 + face.ascender * g.font_size;
        let mut y = unit;
        let mut out = Vec::new();
        for b in blocks {
            let st = layout::block_style(b, g).para;
            for line in layout_paragraph(&b.spans, b.lang, &st, g.text_width(), &self.fonts, &self.hy) {
                out.push(PlacedLine { x: g.margin_x + line.x, baseline: y + base_in, runs: line.runs, block: 0, start: 0, spacing: None, hyphen: false });
                y += unit;
            }
        }
        Some((out, y + unit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_keeps_full_context_styles_and_layout() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fonts");
        let fonts = FontSet::from_prefix(&format!("{root}/PTSerif"), &format!("{root}/LiberationMono-Regular.ttf")).unwrap();
        let mut g = Geometry::new(576.0, 1280.0, 62.0, fonts.faces[0].x_height, false);
        g.bionic = true;
        let b = Book::open("<FictionBook><body><p>Текст до.</p><p>Полный абзац с <emphasis>выделением</emphasis> и продолжением.</p><p>Текст после.</p></body></FictionBook>".as_bytes(), fonts, g).unwrap();
        let report = b.typography_report(1).unwrap();
        for expected in ["Текст до.", "Полный абзац", "продолжением.", "Текст после.", "italic: true", "bionic: true", "offset=", "baseline=", "face="] {
            assert!(report.contains(expected), "нет данных: {expected}");
        }
        assert!(b.typography_report(usize::MAX).is_none());
    }

    #[test]
    fn selection_tracks_source_through_bionic_hyphenation_and_utf16() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fonts");
        let sample = include_str!("../../../tests/fixtures/selection-ru.txt").trim();
        for scroll in [false, true] {
            for bionic in [false, true] {
                let fonts = FontSet::from_prefix(&format!("{root}/PTSerif"), &format!("{root}/LiberationMono-Regular.ttf")).unwrap();
                let g = Geometry { width: 1080.0, height: 2400.0, margin_x: 88.0,
                    margin_y: 150.0, font_size: 48.250492, leading: 1.3347014, scroll, bionic };
                // Extra blocks check block filtering; emoji checks Android's UTF-16 offsets.
                let xml = format!("<FictionBook><body><p>До.</p><p>{sample} 😀</p><p>После.</p></body></FictionBook>");
                let b = Book::open(xml.as_bytes(), fonts, g).unwrap();
                let text = b.doc.blocks[1].text();
                assert_eq!(text.replace('\u{a0}', " "), format!("{} 😀", sample.replace('\u{a0}', " ")));
                let chars: Vec<char> = text.chars().collect();
                let mut covered = vec![false; chars.len()];
                let mut hyphenated = false;
                let mut faces = std::collections::HashSet::new();
                for n in 0..b.page_count() {
                    let cells = b.selection_cells(n, 1);
                    assert_eq!(cells.lines().count(), b.page(n).unwrap().lines.iter()
                        .filter(|l| l.block == 1).flat_map(|l| &l.runs)
                        .flat_map(|r| &r.glyphs).filter(|g| g.source.is_some()).count());
                    for row in cells.lines() {
                        let v: Vec<&str> = row.split_whitespace().collect();
                        let a: usize = v[0].parse().unwrap();
                        let z: usize = v[1].parse().unwrap();
                        let utf16: Vec<u16> = text.encode_utf16().collect();
                        assert!(z > a && z <= utf16.len());
                        assert!(String::from_utf16(&utf16[a..z]).is_ok());
                        assert!(v[4].parse::<f32>().unwrap() >= v[2].parse::<f32>().unwrap());
                    }
                    for l in b.page(n).unwrap().lines.iter().filter(|l| l.block == 1) {
                        hyphenated |= l.hyphen;
                        for r in &l.runs {
                            faces.insert(r.face);
                            for glyph in &r.glyphs {
                                if let Some((a, z)) = glyph.source {
                                    assert!(a < z && z as usize <= chars.len());
                                    covered[a as usize..z as usize].fill(true);
                                }
                            }
                        }
                        if l.hyphen {
                            assert!(l.runs.last().unwrap().glyphs.last().unwrap().source.is_none());
                        }
                    }
                }
                assert!(hyphenated, "fixture must exercise discretionary hyphens");
                if bionic { assert!(faces.len() >= 2); }
                for (i, c) in chars.iter().enumerate() {
                    assert!(c.is_whitespace() || covered[i], "missing source character {i}: {c}");
                }
                assert!(b.selection_cells(usize::MAX, 1).is_empty());
                assert!(b.selection_cells(0, usize::MAX).is_empty());
            }
        }
    }

    #[test]
    fn opens_zipped_fb2() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus/synthetic/04_zipped.fb2.zip"))
            .unwrap();
        let doc = parse(&bytes).unwrap();
        assert!(doc.blocks.len() > 10);
    }
}
