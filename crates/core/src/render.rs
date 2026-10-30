//! Растеризация страницы в картинку — для эталонных тестов на компьютере.
//! На телефоне глифы рисует Android (Canvas.drawGlyphs), здесь — tiny-skia.

use crate::font::{ttf_parser, FontSet};
use crate::layout::text_line;
use crate::para::{Align, GlyphRun};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    /// Строка статуса.
    pub secondary: [u8; 3],
    /// Фон попапа сноски — на шаг темнее (днём) или светлее (ночью) страницы.
    pub panel: [u8; 3],
    /// Коралловый акцент Claude: только элементы интерфейса, не текст книги.
    pub accent: [u8; 3],
}

/// Палитра Claude, подстроенная для чтения. День: кремовый холст Claude и тёплые
/// чернила — контраст почти как у Kindle.
pub const DAY: Theme = Theme {
    bg: [0xFA, 0xF9, 0xF5],
    fg: [0x14, 0x14, 0x13],
    secondary: [0x8E, 0x8B, 0x82],
    panel: [0xF5, 0xF0, 0xE8],
    accent: [0xCC, 0x78, 0x5C],
};

/// Ночь: тёмная поверхность Claude вместо чистого чёрного (на AMOLED меньше
/// смаза и ореолов), текст — приглушённый кремовый, чтобы не слепить в темноте.
pub const NIGHT: Theme = Theme {
    bg: [0x18, 0x17, 0x15],
    fg: [0xCF, 0xCA, 0xC0],
    secondary: [0x75, 0x72, 0x6B],
    panel: [0x25, 0x23, 0x20],
    accent: [0xCC, 0x78, 0x5C],
};

struct Sink<'a> {
    pb: &'a mut PathBuilder,
    x: f32,
    y: f32,
    /// Масштаб по горизонтали (кегль × микротипографика) и по вертикали.
    kx: f32,
    k: f32,
}

impl ttf_parser::OutlineBuilder for Sink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.x + x * self.kx, self.y - y * self.k);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.x + x * self.kx, self.y - y * self.k);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(self.x + x1 * self.kx, self.y - y1 * self.k, self.x + x * self.kx, self.y - y * self.k);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.x + x1 * self.kx,
            self.y - y1 * self.k,
            self.x + x2 * self.kx,
            self.y - y2 * self.k,
            self.x + x * self.kx,
            self.y - y * self.k,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

pub fn canvas(width: u32, height: u32, theme: &Theme) -> Pixmap {
    let mut pix = Pixmap::new(width, height).expect("размер страницы");
    pix.fill(Color::from_rgba8(theme.bg[0], theme.bg[1], theme.bg[2], 255));
    pix
}

pub fn draw_runs(pix: &mut Pixmap, fonts: &FontSet, x0: f32, baseline: f32, runs: &[GlyphRun], color: [u8; 3]) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;
    for run in runs {
        let face = &fonts.faces[run.face as usize];
        let k = run.size / face.upem;
        let mut pb = PathBuilder::new();
        for g in &run.glyphs {
            let mut sink = Sink { pb: &mut pb, x: x0 + g.x, y: baseline + g.y, kx: k * run.xscale, k };
            face.ttf.outline_glyph(ttf_parser::GlyphId(g.id), &mut sink);
        }
        if let Some(path) = pb.finish() {
            pix.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
        if run.strike {
            let y = baseline - 0.28 * run.size;
            let h = (run.size * 0.05).max(1.0);
            if let Some(r) = Rect::from_xywh(x0 + run.x0, y, run.x1 - run.x0, h) {
                pix.fill_rect(r, &paint, Transform::identity(), None);
            }
        }
    }
}

/// Страница книги целиком: строки текста и строка статуса в нижнем поле.
pub fn page(book: &crate::book::Book, n: usize, theme: &Theme) -> Pixmap {
    let g = &book.g;
    let mut pix = canvas(g.width as u32, g.height as u32, theme);
    let Some(page) = book.page(n) else { return pix };
    for l in &page.lines {
        draw_runs(&mut pix, &book.fonts, l.x, l.baseline, &l.runs, theme.fg);
    }
    let (left, right) = book.footer(n);
    draw_footer(&mut pix, book, &left, &right, g.height - g.margin_y, theme);
    pix
}

/// Прокрутка: кусок главы `n` высотой `height` начиная с высоты `top`.
pub fn strip(book: &crate::book::Book, n: usize, top: f32, height: u32, theme: &Theme) -> Pixmap {
    let g = &book.g;
    let mut pix = canvas(g.width as u32, height.max(1), theme);
    let Some(page) = book.page(n) else { return pix };
    let pad = g.unit() * 2.0;
    for l in page.lines.iter().filter(|l| l.baseline > top - pad && l.baseline < top + height as f32 + pad) {
        draw_runs(&mut pix, &book.fonts, l.x, l.baseline - top, &l.runs, theme.fg);
    }
    pix
}

/// Прокрутка: полоса строки статуса (высота — нижнее поле), лежит поверх текста.
pub fn footer_band(book: &crate::book::Book, n: usize, top: f32, theme: &Theme) -> Pixmap {
    let g = &book.g;
    let mut pix = canvas(g.width as u32, g.margin_y.ceil() as u32, theme);
    let (left, right) = book.scroll_footer(n, top);
    draw_footer(&mut pix, book, &left, &right, 0.0, theme);
    pix
}

/// Строка статуса посреди нижнего поля, верх которого на высоте `band_top`.
fn draw_footer(pix: &mut Pixmap, book: &crate::book::Book, left: &str, right: &str, band_top: f32, theme: &Theme) {
    let g = &book.g;
    let size = g.footer_size();
    let y = band_top + g.margin_y / 2.0 + size * 0.35;
    for (text, align) in [(left, Align::Left), (right, Align::Right)] {
        if let Some(l) = text_line(text, size, align, g.text_width(), &book.fonts, &book.hy, book.doc.lang) {
            draw_runs(pix, &book.fonts, g.margin_x + l.x, y, &l.runs, theme.secondary);
        }
    }
}

/// Попап сноски: текст на фоне панели, сверху — тонкая коралловая линия.
pub fn note(book: &crate::book::Book, id: &str, theme: &Theme) -> Option<Pixmap> {
    let (lines, height) = book.note(id)?;
    let g = &book.g;
    let panel = Theme { bg: theme.panel, ..*theme };
    let mut pix = canvas(g.width as u32, height.ceil() as u32, &panel);
    let mut paint = Paint::default();
    paint.set_color_rgba8(theme.accent[0], theme.accent[1], theme.accent[2], 255);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, g.width, (g.font_size * 0.07).max(2.0)) {
        pix.fill_rect(r, &paint, Transform::identity(), None);
    }
    for l in &lines {
        draw_runs(&mut pix, &book.fonts, l.x, l.baseline, &l.runs, theme.fg);
    }
    Some(pix)
}
