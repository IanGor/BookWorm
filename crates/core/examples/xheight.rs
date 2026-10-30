//! Метрики гарнитур: высота строчных, выносные элементы, ширина текста.
use bookworm_core::font::{families, Face};
use bookworm_core::model::Lang;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    for fam in families(&paths) {
        let f = Face::from_file_weight(&fam.faces[0].path, fam.faces[0].wght).unwrap();
        let w: f32 = f.shape("съешь же ещё этих мягких французских булок", 100.0, Lang::Ru).iter().map(|g| g.advance).sum();
        println!(
            "{:<14} x={:.3} asc={:.3} desc={:.3} ширина/x={:.0} кир={} {:?}",
            fam.name, f.x_height, f.ascender, f.descender, w / f.x_height, fam.cyrillic,
            fam.faces.iter().map(|s| format!("{}{}", s.path.rsplit('/').next().unwrap(), s.wght.map_or(String::new(), |w| format!("@{w}")))).collect::<Vec<_>>()
        );
    }
}
