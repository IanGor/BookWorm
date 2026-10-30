//! Равномерность межсловных пробелов: книги корпуса × гарнитуры × размеры.
//! cargo run --release -p bookworm-core --example spacing
//!
//! Для каждой выключенной строки (не последней в абзаце) берётся ширина межсловного
//! пробела относительно обычной. Печатная книга: почти все строки в 0,85–1,2,
//! соседние строки отличаются мало.

use bookworm_core::book::Book;
use bookworm_core::font::{families, FontSet};
use bookworm_core::layout::Geometry;

fn pct(v: &[f32], p: f32) -> f32 {
    v[((v.len() - 1) as f32 * p) as usize]
}

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    // Встроенные гарнитуры и, если положены, посторонние из tests/fonts/<имя> (только для замеров).
    let mut dirs = vec![format!("{root}/fonts")];
    dirs.extend(std::fs::read_dir(format!("{root}/tests/fonts")).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path().display().to_string())));
    let fonts: Vec<String> = dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten())
        .filter_map(|e| e.ok().map(|e| e.path().display().to_string()))
        .filter(|p| p.ends_with(".ttf"))
        .collect();
    let fams = families(&fonts);
    let mono = format!("{root}/fonts/LiberationMono-Regular.ttf");
    // Настоящая проза: синтетика нарочно набита словами-чудовищами и искажает картину.
    let books = ["real/pushkin_pikovaya_dama.fb2", "real/gogol_shinel.fb2", "real/austen_pride_ch1-10.fb2"];
    println!("{:<12} {:>4} {:>6} {:>5} {:>5} {:>5} {:>5} {:>6} {:>6} {:>5}", "гарнитура", "кегль", "строк",
        "p5", "p50", "p95", "max", ">1.25%", "Δсосед", "перен");
    for fam in fams.iter().filter(|f| f.name != "Liberation Mono") {
        for step in [46.0, 62.0, 76.0] {
            let mut sp = Vec::new();
            let mut deltas = Vec::new();
            let (mut lines, mut hyph) = (0usize, 0usize);
            for name in books {
                let bytes = std::fs::read(format!("{root}/tests/corpus/{name}")).unwrap();
                let set = FontSet::from_specs(&fam.faces, &mono).unwrap();
                let g = Geometry::new(1080.0, 2400.0, step, set.faces[0].x_height, true);
                let b = Book::open(&bytes, set, g).unwrap();
                for n in 0..b.page_count() {
                    let mut prev: Option<f32> = None;
                    for l in &b.page(n).unwrap().lines {
                        lines += 1;
                        hyph += l.hyphen as usize;
                        match l.spacing {
                            Some(s) => {
                                sp.push(s);
                                if let Some(p) = prev {
                                    deltas.push((s - p).abs());
                                }
                                prev = Some(s);
                            }
                            None => prev = None,
                        }
                    }
                }
            }
            sp.sort_by(f32::total_cmp);
            let loose = sp.iter().filter(|&&s| s > 1.25 || s < 0.8).count() as f32 / sp.len() as f32 * 100.0;
            let dmean = deltas.iter().sum::<f32>() / deltas.len().max(1) as f32;
            println!(
                "{:<12} {:>4} {:>6} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>6.1} {:>6.3} {:>4.1}%",
                fam.name, step, sp.len(), pct(&sp, 0.05), pct(&sp, 0.5), pct(&sp, 0.95), sp[sp.len() - 1], loose, dmean,
                hyph as f32 / lines as f32 * 100.0
            );
        }
    }
}
