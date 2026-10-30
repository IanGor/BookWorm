//! bw-render: книга → PNG-страницы, как их сверстает читалка на телефоне.
//!
//! bw-render КНИГА [--out DIR] [--pages A-B] [--size PX] [--night]
//!                 [--font ПРЕФИКС] [--mono ФАЙЛ] [--note ID]
//!                 [--family ГАРНИТУРА] [--scroll]
//! --family берёт гарнитуру из папки fonts/ (Literata, PT Serif, Charis SIL, PT Sans);
//! --size — ступень размера (кегль опорного шрифта), кегль гарнитуры подбирается по высоте строчных.
//! Пример: bw-render tests/corpus/synthetic/01_typography_ru.fb2 --pages 1-5

use bookworm_core::{book::Book, font, font::FontSet, layout, render};
use std::path::PathBuf;
use std::time::Instant;

const USAGE: &str =
    "bw-render КНИГА [--out DIR] [--pages A-B] [--size PX] [--night] [--font ПРЕФИКС] [--mono ФАЙЛ] [--note ID] [--family ИМЯ] [--scroll]";
const DEFAULT_FONT: &str = "/usr/share/fonts/noto/NotoSerif";
const DEFAULT_MONO: &str = "/usr/share/fonts/liberation/LiberationMono-Regular.ttf";

struct Args {
    book: PathBuf,
    out: PathBuf,
    pages: Option<(usize, usize)>,
    font: String,
    mono: String,
    size: f32,
    night: bool,
    note: Option<String>,
    family: Option<String>,
    scroll: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        book: PathBuf::new(),
        out: PathBuf::from("out"),
        pages: None,
        font: DEFAULT_FONT.into(),
        mono: DEFAULT_MONO.into(),
        size: 46.0,
        night: false,
        note: None,
        family: None,
        scroll: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => a.out = it.next().ok_or("--out: нужен путь")?.into(),
            "--font" => a.font = it.next().ok_or("--font: нужен префикс пути")?,
            "--mono" => a.mono = it.next().ok_or("--mono: нужен файл")?,
            "--size" => a.size = it.next().ok_or("--size: нужно число")?.parse().map_err(|_| "--size: нужно число")?,
            "--night" => a.night = true,
            "--note" => a.note = Some(it.next().ok_or("--note: нужен id сноски")?),
            "--family" => a.family = Some(it.next().ok_or("--family: нужно имя гарнитуры")?),
            "--scroll" => a.scroll = true,
            "--pages" => {
                let s = it.next().ok_or("--pages: нужен диапазон A-B")?;
                let (x, y) = s.split_once('-').unwrap_or((s.as_str(), s.as_str()));
                let x: usize = x.parse().map_err(|_| "--pages: нужен диапазон A-B")?;
                let y: usize = y.parse().map_err(|_| "--pages: нужен диапазон A-B")?;
                a.pages = Some((x.max(1), y.max(x)));
            }
            _ if a.book.as_os_str().is_empty() && !arg.starts_with("--") => a.book = arg.into(),
            _ => return Err(format!("неизвестный аргумент {arg}\n{USAGE}")),
        }
    }
    if a.book.as_os_str().is_empty() {
        return Err(USAGE.into());
    }
    Ok(a)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ошибка: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let a = parse_args()?;
    let bytes = std::fs::read(&a.book).map_err(|e| format!("{}: {e}", a.book.display()))?;
    let fonts = match &a.family {
        Some(name) => {
            let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fonts");
            let files: Vec<String> = std::fs::read_dir(dir)
                .map_err(|e| format!("{dir}: {e}"))?
                .filter_map(|e| e.ok().map(|e| e.path().display().to_string()))
                .filter(|p| p.ends_with(".ttf") || p.ends_with(".otf"))
                .collect();
            let fams = font::families(&files);
            let fam = fams.iter().find(|f| f.name.eq_ignore_ascii_case(name)).ok_or_else(|| {
                format!("нет гарнитуры {name}; есть: {}", fams.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", "))
            })?;
            FontSet::from_specs(&fam.faces, &a.mono)?
        }
        None => FontSet::from_prefix(&a.font, &a.mono)?,
    };
    let g = layout::Geometry::new(1080.0, 2400.0, a.size, fonts.faces[0].x_height, a.scroll);
    let t = Instant::now();
    let book = Book::open(&bytes, fonts, g)?;
    let doc = &book.doc;
    eprintln!(
        "«{}»: блоков {}, сносок {}, глав {}, страниц {} (строк на странице {}); разбор и вёрстка {:.0?}",
        doc.title,
        doc.blocks.len(),
        doc.notes.len(),
        doc.toc.len(),
        book.page_count(),
        book.g.lines_per_page(),
        t.elapsed()
    );
    if book.page_count() == 0 {
        return Ok(());
    }

    std::fs::create_dir_all(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?;
    let theme = if a.night { render::NIGHT } else { render::DAY };
    if let Some(id) = &a.note {
        let pix = render::note(&book, id, &theme).ok_or(format!("сноски {id} нет"))?;
        let path = a.out.join(format!("note-{}.png", id.replace(['/', '#'], "_")));
        pix.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("попап сноски сохранён в {}", path.display());
        return Ok(());
    }
    if a.scroll {
        // Первые экраны прокрутки первой главы с текстом, строка статуса — поверх.
        let n = (0..book.page_count()).find(|&i| book.page(i).is_some_and(|p| p.lines.len() > 3)).unwrap_or(0);
        let h = book.g.height as u32;
        for k in 0..3u32 {
            let top = k as f32 * book.g.height * 0.75;
            let mut pix = render::strip(&book, n, top, h, &theme);
            let band = render::footer_band(&book, n, top, &theme);
            pix.draw_pixmap(0, (h - band.height()) as i32, band.as_ref(), &Default::default(), Default::default(), None);
            let path = a.out.join(format!("scroll-{}.png", k + 1));
            pix.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        eprintln!("экраны прокрутки сохранены в {}", a.out.display());
        return Ok(());
    }
    let (from, to) = a.pages.unwrap_or((1, book.page_count().min(20)));
    let to = to.min(book.page_count());
    for n in from..=to {
        let pix = render::page(&book, n - 1, &theme);
        let path = a.out.join(format!("page-{n:04}.png"));
        pix.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    eprintln!("страницы {from}–{to} сохранены в {}", a.out.display());
    Ok(())
}
