//! Отладка одного абзаца: строки с шириной пробела и переносом.
use bookworm_core::book::Book;
use bookworm_core::font::{families, FontSet};
use bookworm_core::layout::Geometry;

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let args: Vec<String> = std::env::args().collect();
    let fam_name = args.get(1).map_or("Literata", |s| s.as_str());
    let step: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(46.0);
    let fonts: Vec<String> = std::fs::read_dir(format!("{root}/fonts")).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path().display().to_string())).filter(|p| p.ends_with(".ttf")).collect();
    let fam = families(&fonts).into_iter().find(|f| f.name == fam_name).unwrap();
    let set = FontSet::from_specs(&fam.faces, &format!("{root}/fonts/LiberationMono-Regular.ttf")).unwrap();
    let g = Geometry::new(1080.0, 2400.0, step, set.faces[0].x_height, true);
    println!("кегль {:.1}, пробел {:.1}", g.font_size, set.metrics(0, g.font_size).space);
    let bytes = std::fs::read(format!("{root}/tests/corpus/real/pushkin_pikovaya_dama.fb2")).unwrap();
    let b = Book::open(&bytes, set, g).unwrap();
    let p = b.page(0).unwrap();
    let block = p.lines.iter().map(|l| l.block).find(|&bl| b.doc.blocks[bl].text().len() > 600).unwrap();
    let chars: Vec<char> = b.doc.blocks[block].text().chars().collect();
    let lines: Vec<_> = p.lines.iter().filter(|l| l.block == block).collect();
    for (i, l) in lines.iter().enumerate() {
        let end = lines.get(i + 1).map_or(chars.len(), |n| n.start as usize);
        let text: String = chars[l.start as usize..end].iter().map(|&c| if c == '\u{a0}' { '~' } else { c }).collect();
        println!("{:>5} {} |{}|", l.spacing.map_or("  -  ".into(), |s| format!("{s:.2}")), if l.hyphen { "-" } else { " " }, text);
    }
}
