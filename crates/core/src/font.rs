//! Шрифты: загрузка начертаний, шейпинг и метрики. Ядро всегда работает
//! с файлами шрифтов, поэтому раскладка одинакова на компьютере и телефоне.

use crate::model::{InlineStyle, Lang};
pub use rustybuzz::ttf_parser;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

pub const REGULAR: usize = 0;
pub const ITALIC: usize = 1;
pub const BOLD: usize = 2;
pub const BOLD_ITALIC: usize = 3;
pub const MONO: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub id: u16,
    /// Байтовое смещение начала кластера в исходной строке.
    pub cluster: usize,
    pub advance: f32,
    pub dx: f32,
    /// Смещение по вертикали вниз, в пикселях.
    pub dy: f32,
}

pub struct Face {
    pub rb: rustybuzz::Face<'static>,
    pub ttf: ttf_parser::Face<'static>,
    pub upem: f32,
    /// Доли em, оба положительные.
    pub ascender: f32,
    pub descender: f32,
    /// Высота строчных в долях em: по ней подбирается кегль (SPEC §2.8).
    pub x_height: f32,
}

/// Данные файлов шрифтов живут до конца процесса и загружаются один раз на файл:
/// смена шрифта или переоткрытие книги не копят память.
fn font_data(path: &str) -> Result<&'static [u8], String> {
    static CACHE: Mutex<Option<HashMap<String, &'static [u8]>>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(d) = map.get(path) {
        return Ok(d);
    }
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let data: &'static [u8] = Box::leak(data.into_boxed_slice());
    map.insert(path.to_string(), data);
    Ok(data)
}

const WGHT: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"wght");

impl Face {
    pub fn from_bytes(data: Vec<u8>) -> Result<Face, String> {
        Face::from_static(Box::leak(data.into_boxed_slice()), None)
    }

    pub fn from_file(path: &str) -> Result<Face, String> {
        Face::from_static(font_data(path)?, None)
    }

    /// Начертание вариативного шрифта: `wght` — насыщенность (400 — обычный, 700 — жирный).
    pub fn from_file_weight(path: &str, wght: Option<f32>) -> Result<Face, String> {
        Face::from_static(font_data(path)?, wght)
    }

    fn from_static(data: &'static [u8], wght: Option<f32>) -> Result<Face, String> {
        let mut ttf = ttf_parser::Face::parse(data, 0).map_err(|e| format!("шрифт не читается: {e}"))?;
        let mut rb = rustybuzz::Face::from_slice(data, 0).ok_or("шейпер не принял шрифт")?;
        if let Some(w) = wght {
            ttf.set_variation(WGHT, w);
            rb.set_variations(&[rustybuzz::Variation { tag: WGHT, value: w }]);
        }
        let upem = ttf.units_per_em() as f32;
        let x_height = ttf
            .x_height()
            .filter(|&x| x > 0)
            .map(|x| x as f32)
            .or_else(|| {
                let g = ttf.glyph_index('x')?;
                ttf.glyph_bounding_box(g).map(|b| b.y_max as f32)
            })
            .map_or(0.5, |x| x / upem);
        Ok(Face {
            ascender: ttf.ascender() as f32 / upem,
            descender: -(ttf.descender() as f32) / upem,
            upem,
            x_height,
            rb,
            ttf,
        })
    }

    pub fn shape(&self, text: &str, size: f32, lang: Lang) -> Vec<Glyph> {
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        buf.set_direction(rustybuzz::Direction::LeftToRight);
        if let Ok(l) = rustybuzz::Language::from_str(lang.code()) {
            buf.set_language(l);
        }
        buf.guess_segment_properties();
        let out = rustybuzz::shape(&self.rb, &[], buf);
        let k = size / self.upem;
        out.glyph_infos()
            .iter()
            .zip(out.glyph_positions())
            .map(|(i, p)| Glyph {
                id: i.glyph_id as u16,
                cluster: i.cluster as usize,
                advance: p.x_advance as f32 * k,
                dx: p.x_offset as f32 * k,
                dy: -(p.y_offset as f32) * k,
            })
            .collect()
    }
}

/// Ширина пробела и глифы дефиса для начертания и кегля.
#[derive(Clone)]
pub struct Metrics {
    pub space: f32,
    pub hyphen: Vec<Glyph>,
}

pub struct FontSet {
    /// REGULAR, ITALIC, BOLD, BOLD_ITALIC, MONO; затем дополнительные начертания.
    pub faces: Vec<Face>,
    bionic_faces: [usize; 2],
    cache: Mutex<HashMap<(usize, u32), Metrics>>,
}

impl FontSet {
    pub fn new(mut faces: Vec<Face>) -> FontSet {
        let mut bionic_faces = [BOLD, BOLD_ITALIC];
        for (slot, base) in [REGULAR, ITALIC].into_iter().enumerate() {
            let f = &faces[base];
            if f.ttf.variation_axes().into_iter().any(|a| a.tag == WGHT) {
                if let Ok(face) = Face::from_static(f.ttf.raw_face().data, Some(600.0)) {
                    bionic_faces[slot] = faces.len();
                    faces.push(face);
                }
            }
        }
        FontSet { faces, bionic_faces, cache: Mutex::new(HashMap::new()) }
    }

    /// Semibold у вариативных гарнитур, доступный жирный у статических.
    /// Авторский жирный и моноширинный код сохраняют своё начертание.
    pub fn bionic_face(&self, base: usize) -> usize {
        match base {
            REGULAR => self.bionic_faces[0],
            ITALIC => self.bionic_faces[1],
            _ => base,
        }
    }

    /// `prefix` — путь без начертания: `/usr/share/fonts/noto/NotoSerif`
    /// → `NotoSerif-Regular.ttf`, `-Italic`, `-Bold`, `-BoldItalic`.
    pub fn from_prefix(prefix: &str, mono: &str) -> Result<FontSet, String> {
        let load = |suffix: &str| Face::from_file(&format!("{prefix}-{suffix}.ttf"));
        Ok(FontSet::new(vec![
            load("Regular")?,
            load("Italic")?,
            load("Bold")?,
            load("BoldItalic")?,
            Face::from_file(mono)?,
        ]))
    }

    /// Начертания по описаниям: обычное, курсив, жирный, жирный курсив; затем моноширинный.
    pub fn from_specs(faces: &[FaceSpec; 4], mono: &str) -> Result<FontSet, String> {
        let mut out = Vec::with_capacity(5);
        for f in faces {
            out.push(Face::from_file_weight(&f.path, f.wght)?);
        }
        out.push(Face::from_file(mono)?);
        Ok(FontSet::new(out))
    }

    pub fn face_index(style: &InlineStyle) -> usize {
        if style.code {
            return MONO;
        }
        match (style.bold, style.italic) {
            (false, false) => REGULAR,
            (false, true) => ITALIC,
            (true, false) => BOLD,
            (true, true) => BOLD_ITALIC,
        }
    }

    pub fn metrics(&self, face: usize, size: f32) -> Metrics {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry((face, size.to_bits()))
            .or_insert_with(|| {
                let f = &self.faces[face];
                Metrics {
                    space: f.shape(" ", size, Lang::Ru).iter().map(|g| g.advance).sum(),
                    hyphen: f.shape("-", size, Lang::Ru),
                }
            })
            .clone()
    }
}

/// Файл начертания и насыщенность, если шрифт вариативный.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceSpec {
    pub path: String,
    pub wght: Option<f32>,
}

impl FaceSpec {
    /// Запись `путь|насыщенность` (0 — статический шрифт).
    pub fn parse(s: &str) -> Option<FaceSpec> {
        let (path, w) = s.rsplit_once('|').unwrap_or((s, "0"));
        let w: f32 = w.trim().parse().ok()?;
        Some(FaceSpec { path: path.trim().to_string(), wght: (w > 0.0).then_some(w) })
    }

    pub fn encode(&self) -> String {
        format!("{}|{}", self.path, self.wght.unwrap_or(0.0))
    }
}

/// Гарнитура: четыре начертания (недостающие заменены ближайшими).
#[derive(Clone, Debug)]
pub struct Family {
    pub name: String,
    pub faces: [FaceSpec; 4],
    /// Есть ли русские буквы: без них книга на русском будет пустыми квадратами.
    pub cyrillic: bool,
}

struct FileInfo {
    path: String,
    family: String,
    weight: u16,
    italic: bool,
    variable: bool,
    cyrillic: bool,
}

fn file_info(path: &str) -> Option<FileInfo> {
    let data = font_data(path).ok()?;
    let f = ttf_parser::Face::parse(data, 0).ok()?;
    let name = |id: u16| {
        f.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string())
    };
    let family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| name(ttf_parser::name_id::FAMILY))?;
    let variable = f.variation_axes().into_iter().any(|a| a.tag == WGHT);
    Some(FileInfo {
        path: path.to_string(),
        family,
        weight: f.weight().to_number(),
        italic: f.is_italic() || f.is_oblique(),
        variable,
        cyrillic: ['ж', 'Я', 'ё'].iter().all(|&c| f.glyph_index(c).is_some()),
    })
}

/// Гарнитуры из файлов шрифтов (TTF/OTF): начертания группируются по семейству.
/// Вариативный шрифт даёт обычное и жирное начертание из одного файла.
pub fn families(paths: &[String]) -> Vec<Family> {
    let mut by_name: Vec<(String, Vec<FileInfo>)> = Vec::new();
    for p in paths {
        let Some(info) = file_info(p) else { continue };
        match by_name.iter_mut().find(|(n, _)| *n == info.family) {
            Some((_, v)) => v.push(info),
            None => by_name.push((info.family.clone(), vec![info])),
        }
    }
    let mut out = Vec::new();
    for (name, files) in by_name {
        // Ближайший файл к нужной насыщенности и наклону; вариативный — с нужной насыщенностью.
        let pick = |want_w: u16, want_i: bool| -> Option<FaceSpec> {
            let f = files
                .iter()
                .filter(|f| f.italic == want_i)
                .min_by_key(|f| if f.variable { 0 } else { (f.weight as i32 - want_w as i32).unsigned_abs() + 1 })?;
            Some(FaceSpec { path: f.path.clone(), wght: f.variable.then_some(want_w as f32) })
        };
        let Some(regular) = pick(400, false).or_else(|| pick(400, true)) else { continue };
        let bold = pick(700, false).filter(|b| *b != regular || b.wght.is_some()).unwrap_or_else(|| regular.clone());
        let italic = pick(400, true).unwrap_or_else(|| regular.clone());
        let bold_italic = pick(700, true)
            .filter(|b| *b != italic || b.wght.is_some())
            .unwrap_or_else(|| bold.clone());
        let cyrillic = files.iter().all(|f| f.cyrillic);
        out.push(Family { name, faces: [regular, italic, bold, bold_italic], cyrillic });
    }
    out
}
