//! Переносы по словарям TeX (hyph-utf8) алгоритмом Лианга.
//! Правила: docs/SPEC.md, §2.4. Ограничение «не больше двух переносов подряд»
//! и штрафы — в para.rs.

use crate::model::Lang;
use hyphenation::{Hyphenator as _, Language, Load, Standard};

pub struct Hyphenator {
    ru: Standard,
    en: Standard,
}

impl Default for Hyphenator {
    fn default() -> Self {
        Self::new()
    }
}

impl Hyphenator {
    pub fn new() -> Hyphenator {
        Hyphenator {
            ru: Standard::from_embedded(Language::Russian).expect("встроенный словарь ru"),
            en: Standard::from_embedded(Language::EnglishUS).expect("встроенный словарь en-us"),
        }
    }

    /// Индексы символов слова, перед которыми допустим перенос.
    /// Слово — только буквы одного алфавита и знаки ударения.
    pub fn points(&self, word: &[char], lang: Lang) -> Vec<usize> {
        let (dict, lmin, rmin) = match lang {
            Lang::Ru => (&self.ru, 2, 2),
            Lang::En => (&self.en, 2, 3),
        };
        let mut letters = String::new();
        let mut map = Vec::new();
        for (i, &c) in word.iter().enumerate() {
            if is_combining(c) {
                continue;
            }
            letters.extend(c.to_lowercase());
            map.push(i);
        }
        let n = map.len();
        // Короткие слова и аббревиатуры не переносим; длинное слово заглавными
        // («ЗАМЕЧАТЕЛЬНАЯ») — не аббревиатура, его переносить можно.
        let caps = !word.iter().any(|c| c.is_lowercase());
        if n < 5 || (caps && n < 8) || letters.chars().count() != n {
            return Vec::new();
        }
        let breaks = dict.hyphenate(&letters).breaks;
        breaks
            .into_iter()
            .map(|b| letters[..b].chars().count())
            .filter(|&k| k >= lmin && n - k >= rmin)
            .map(|k| map[k])
            .collect()
    }
}

pub fn is_combining(c: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&c)
}

fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

/// По каким правилам переносить слово. Латиница в русской книге не переносится:
/// это может быть французский или немецкий.
pub fn script_lang(word: &[char], para: Lang) -> Option<Lang> {
    let letters = word.iter().filter(|c| !is_combining(**c));
    let mut cyr = true;
    let mut lat = true;
    for &c in letters {
        cyr &= is_cyrillic(c);
        lat &= c.is_ascii_alphabetic();
    }
    match (cyr, lat, para) {
        (true, _, _) => Some(Lang::Ru),
        (_, true, Lang::En) => Some(Lang::En),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hyph(h: &Hyphenator, w: &str, lang: Lang) -> String {
        let chars: Vec<char> = w.chars().collect();
        let pts = h.points(&chars, lang);
        let mut s = String::new();
        for (i, c) in chars.iter().enumerate() {
            if pts.contains(&i) {
                s.push('-');
            }
            s.push(*c);
        }
        s
    }

    #[test]
    fn russian_and_english() {
        let h = Hyphenator::new();
        let ru = hyph(&h, "достопримечательность", Lang::Ru);
        assert!(ru.contains('-'), "{ru}");
        assert!(!ru.starts_with("д-") && !ru.ends_with("-ь"), "{ru}");
        let en = hyph(&h, "hyphenation", Lang::En);
        assert!(en.contains('-'), "{en}");
        assert_eq!(hyph(&h, "ООН", Lang::Ru), "ООН");
    }

    #[test]
    fn stress_marks_are_kept() {
        let h = Hyphenator::new();
        let w = "водопрово́дный";
        let s = hyph(&h, w, Lang::Ru);
        assert_eq!(s.replace('-', ""), w);
    }

    #[test]
    fn script() {
        let w: Vec<char> = "Мoсква".chars().collect();
        assert_eq!(script_lang(&w, Lang::Ru), None);
        let w: Vec<char> = "question".chars().collect();
        assert_eq!(script_lang(&w, Lang::Ru), None);
        assert_eq!(script_lang(&w, Lang::En), Some(Lang::En));
    }
}
