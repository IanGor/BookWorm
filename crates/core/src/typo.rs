//! Типограф: превращает «грязный» текст из конвертеров в типографски правильный.
//! Работает над абзацем целиком, сохраняя границы инлайн-стилей. Моноширинный
//! код не трогает. Правила — в docs/SPEC.md, §2.3.

use crate::model::{Lang, Span};

pub const NBSP: char = '\u{00A0}';
pub const SHY: char = '\u{00AD}';
pub const ZWSP: char = '\u{200B}';
/// Узкий неразрывный пробел: разряды чисел. Не растягивается при выключке.
pub const NNBSP: char = '\u{202F}';

/// Символ абзаца и номер фрагмента, которому он принадлежит.
#[derive(Clone, Copy)]
struct Ch {
    c: char,
    s: u16,
}

pub fn apply(spans: &mut Vec<Span>, lang: Lang) {
    let mut v: Vec<Ch> = Vec::new();
    for (i, sp) in spans.iter().enumerate() {
        let s = i.min(u16::MAX as usize) as u16;
        v.extend(sp.text.chars().map(|c| Ch { c, s }));
    }
    let code: Vec<bool> = spans.iter().map(|s| s.style.code).collect();
    let v = clean(v);
    let v = ellipsis(v, &code);
    let v = dashes(v, lang, &code);
    let v = quotes(v, lang, &code);
    let v = if lang == Lang::Ru { nbsp_ru(v, &code) } else { v };
    *spans = rebuild(&v, spans);
}

fn rebuild(v: &[Ch], old: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut cur: Option<u16> = None;
    for ch in v {
        if cur != Some(ch.s) {
            let o = &old[ch.s as usize];
            out.push(Span { text: String::new(), style: o.style, note: o.note.clone() });
            cur = Some(ch.s);
        }
        if let Some(last) = out.last_mut() {
            last.text.push(ch.c);
        }
    }
    out
}

fn is_sp(c: char) -> bool {
    c == ' ' || c == NBSP
}

/// Пробелы: схлопывание, обрезка краёв, экзотические пробелы → обычные.
fn clean(v: Vec<Ch>) -> Vec<Ch> {
    let mut out: Vec<Ch> = Vec::with_capacity(v.len());
    for ch in v {
        let c = match ch.c {
            '\u{2007}' => NBSP,
            '\t' | '\n' | '\r' | '\u{2000}'..='\u{200A}' | '\u{205F}' | '\u{3000}' => ' ',
            '\u{FEFF}' => continue,
            c if (c as u32) < 0x20 => ' ',
            c => c,
        };
        if is_sp(c) {
            match out.last_mut() {
                None => continue,
                Some(l) if l.c == ' ' => {
                    if c == NBSP {
                        l.c = NBSP;
                    }
                    continue;
                }
                Some(l) if l.c == NBSP || l.c == ZWSP => continue,
                _ => {}
            }
        }
        if c == ZWSP && out.last().is_none_or(|l| is_sp(l.c)) {
            continue;
        }
        out.push(Ch { c, s: ch.s });
    }
    while out.last().is_some_and(|l| is_sp(l.c) || l.c == ZWSP) {
        out.pop();
    }
    out
}

/// `...` → `…`; `?..` и `!..` сохраняются; `. . .` → `…`.
fn ellipsis(v: Vec<Ch>, code: &[bool]) -> Vec<Ch> {
    let mut out: Vec<Ch> = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let ch = v[i];
        if code[ch.s as usize] || (ch.c != '.' && ch.c != '…') {
            out.push(ch);
            i += 1;
            continue;
        }
        if ch.c == '.' && spaced_dots(&v, i) {
            out.push(Ch { c: '…', s: ch.s });
            i += 5;
            while i < v.len() && v[i].c == '.' {
                i += 1;
            }
            continue;
        }
        let mut j = i;
        let mut ell = false;
        while j < v.len() && (v[j].c == '.' || v[j].c == '…') {
            ell |= v[j].c == '…';
            j += 1;
        }
        let dots = j - i;
        let prev = out.last().map_or('\0', |c| c.c);
        if (prev == '?' || prev == '!') && !ell {
            out.extend_from_slice(&v[i..i + dots.min(2)]);
        } else if ell || dots >= 3 {
            out.push(Ch { c: '…', s: ch.s });
        } else {
            out.extend_from_slice(&v[i..j]);
        }
        i = j;
    }
    out
}

fn spaced_dots(v: &[Ch], i: usize) -> bool {
    i + 4 < v.len() && v[i + 1].c == ' ' && v[i + 2].c == '.' && v[i + 3].c == ' ' && v[i + 4].c == '.'
}

/// Дефисы и тире: реплики, тире между словами, диапазоны чисел, минус.
fn dashes(v: Vec<Ch>, lang: Lang, code: &[bool]) -> Vec<Ch> {
    let mut out: Vec<Ch> = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let ch = v[i];
        if code[ch.s as usize] || !matches!(ch.c, '-' | '–' | '—') {
            out.push(ch);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if ch.c == '-' {
            while j < v.len() && v[j].c == '-' {
                j += 1;
            }
        }
        let run = j - i;
        let is_long = ch.c == '—' || run >= 2;
        let next = v.get(j).map_or('\0', |c| c.c);
        let prev = out.last().map(|c| c.c);
        let mk = |c| Ch { c, s: ch.s };

        // Тире в начале абзаца — реплика диалога.
        if prev.is_none() && (is_sp(next) || (is_long && next != '\0')) {
            out.push(mk('—'));
            out.push(mk(NBSP));
            i = if is_sp(next) { j + 1 } else { j };
            continue;
        }
        let p = prev.unwrap_or('\0');
        // Тире между словами: « - », « -- », « – ».
        if is_sp(p) && (is_sp(next) || next == '\0') {
            out.pop();
            match lang {
                Lang::Ru => {
                    out.push(mk(NBSP));
                    out.push(mk('—'));
                }
                Lang::En if is_long => {
                    out.push(mk('—'));
                    if is_sp(next) {
                        j += 1;
                    }
                }
                Lang::En => {
                    out.push(mk(NBSP));
                    out.push(mk('–'));
                }
            }
            i = j;
            continue;
        }
        // Короткое тире между буквами без пробелов — ошибка набора вместо дефиса
        // («Само–реализация»): дефис, иначе слово не переносится как составное.
        if ch.c == '–' && run == 1 && p.is_alphabetic() && next.is_alphabetic() {
            out.push(mk('-'));
            i = j;
            continue;
        }
        // Тире вплотную после слова или знака: «инженера,— отроду» → «инженера,\u{a0}— отроду».
        let after_punct = ",;:!?…»)".contains(p);
        if lang == Lang::Ru && is_sp(next) && p != '\0' && !p.is_ascii_digit() && (is_long || (ch.c == '-' && after_punct)) {
            out.push(mk(NBSP));
            out.push(mk('—'));
            i = j;
            continue;
        }
        // Слитное «слово--слово».
        if run >= 2 {
            match lang {
                Lang::Ru => {
                    out.push(mk(NBSP));
                    out.push(mk('—'));
                    out.push(mk(' '));
                }
                Lang::En => out.push(mk('—')),
            }
            i = j;
            continue;
        }
        if ch.c == '-' && p.is_ascii_digit() && next.is_ascii_digit() && is_range(&out, &v, j) {
            out.push(mk('–'));
            i = j;
            continue;
        }
        if ch.c == '-' && (is_sp(p) || p == '\0' || p == '(') && next.is_ascii_digit() {
            out.push(mk('−'));
            i = j;
            continue;
        }
        out.extend_from_slice(&v[i..j]);
        i = j;
    }
    out
}

/// «1812-1814» — диапазон; «123-45-67» и «7.0.11-2011» — нет.
fn is_range(out: &[Ch], v: &[Ch], j: usize) -> bool {
    let left = out.iter().rev().take_while(|c| c.c.is_ascii_digit()).count();
    let before = out.len().checked_sub(left + 1).map(|k| out[k].c);
    let right = v[j..].iter().take_while(|c| c.c.is_ascii_digit()).count();
    let after = v.get(j + right).map(|c| c.c);
    (1..=4).contains(&left)
        && (1..=4).contains(&right)
        && !matches!(before, Some('-' | '–' | '.' | ','))
        && !matches!(after, Some('-' | '–' | '.') if v.get(j + right + 1).is_some_and(|c| c.c.is_ascii_digit()))
}

/// Прямые кавычки → «ёлочки» и „лапки“ (ru) или “ ” ‘ ’ (en) с учётом вложенности.
fn quotes(v: Vec<Ch>, lang: Lang, code: &[bool]) -> Vec<Ch> {
    let (o1, c1, o2, c2) = match lang {
        Lang::Ru => ('«', '»', '„', '“'),
        Lang::En => ('“', '”', '‘', '’'),
    };
    let mut out: Vec<Ch> = Vec::with_capacity(v.len());
    let mut depth = 0usize;
    let mut single = 0usize;
    for i in 0..v.len() {
        let ch = v[i];
        if code[ch.s as usize] {
            out.push(ch);
            continue;
        }
        let prev = out.last().map_or('\0', |c| c.c);
        let next = v.get(i + 1).map_or('\0', |c| c.c);
        let c = match (lang, ch.c) {
            (Lang::Ru, '«') | (_, '„') | (Lang::En, '“') => {
                depth += 1;
                ch.c
            }
            (Lang::Ru, '»') | (Lang::Ru, '“') | (Lang::En, '”') => {
                depth = depth.saturating_sub(1);
                ch.c
            }
            // 5' 10" — футы и дюймы, а не кавычки.
            (_, '"') if prev.is_ascii_digit() && depth == 0 => '″',
            (Lang::Ru, '\'') if prev.is_ascii_digit() => '′',
            (_, '"') => {
                if opens(prev, next) {
                    let q = if depth == 0 { o1 } else { o2 };
                    depth += 1;
                    q
                } else {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        c1
                    } else {
                        c2
                    }
                }
            }
            (Lang::Ru, '\'') => '’',
            (Lang::En, '‘') => {
                single += 1;
                '‘'
            }
            (Lang::En, '\'' | '’') => {
                let pl = prev.is_alphanumeric();
                let nl = next.is_alphanumeric();
                if pl && nl {
                    '’'
                } else if !pl && next.is_ascii_digit() {
                    '’'
                } else if !pl && nl && !elision(&v, i + 1) {
                    single += 1;
                    '‘'
                } else if !pl && nl {
                    '’'
                } else {
                    single = single.saturating_sub(1);
                    '’'
                }
            }
            (_, c) => c,
        };
        out.push(Ch { c, s: ch.s });
    }
    out
}

fn opens(prev: char, next: char) -> bool {
    let prev_open = prev == '\0' || is_sp(prev) || "([{«„“‘—–-/\u{200B}".contains(prev);
    let next_close = next == '\0' || is_sp(next) || ".,;:!?)…".contains(next);
    prev_open && !next_close
}

/// Английские слова с апострофом в начале: 'tis, 'twas, 'em, 'n'.
fn elision(v: &[Ch], from: usize) -> bool {
    let word: String = v[from..].iter().map(|c| c.c).take_while(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
    matches!(word.as_str(), "tis" | "twas" | "em" | "n" | "til" | "cause" | "bout" | "round")
}

/// Висячие предлоги и союзы, частицы.
const SHORT: &[&str] = &[
    "в", "к", "с", "у", "о", "и", "а", "я", "на", "по", "за", "из", "от", "до", "не", "ни", "но", "во", "ко",
    "со", "об",
];
const ENCLITIC: &[&str] = &["ли", "ль", "же", "ж", "бы", "б"];

/// Неразрывные пробелы по правилам русской типографики.
fn nbsp_ru(v: Vec<Ch>, code: &[bool]) -> Vec<Ch> {
    let c: Vec<char> = v.iter().map(|x| x.c).collect();
    let mut out: Vec<Ch> = Vec::with_capacity(v.len() + 8);
    for i in 0..v.len() {
        let ch = v[i];
        if code[ch.s as usize] {
            out.push(ch);
            continue;
        }
        if ch.c == ' ' && thousands(&c, i) {
            out.push(Ch { c: NNBSP, s: ch.s });
            continue;
        }
        if ch.c == ' ' && nbsp_here(&c, i) {
            out.push(Ch { c: NBSP, s: ch.s });
            continue;
        }
        out.push(ch);
        if needs_insert(&c, i) {
            out.push(Ch { c: NBSP, s: ch.s });
        }
    }
    out
}

fn word_before(c: &[char], i: usize) -> String {
    let mut s = i;
    while s > 0 && c[s - 1].is_alphabetic() {
        s -= 1;
    }
    c[s..i].iter().collect::<String>().to_lowercase()
}

fn word_after(c: &[char], i: usize) -> String {
    c[i + 1..].iter().take_while(|x| x.is_alphabetic()).collect::<String>().to_lowercase()
}

fn at(c: &[char], i: usize) -> char {
    c.get(i).copied().unwrap_or('\0')
}

/// Одиночная буква с точкой: «А.», «т.» (перед ней не буква).
fn single_letter_dot(c: &[char], dot: usize) -> Option<char> {
    if dot == 0 || c[dot] != '.' {
        return None;
    }
    let l = c[dot - 1];
    let alone = dot < 2 || !c[dot - 2].is_alphabetic();
    (l.is_alphabetic() && alone).then_some(l)
}

fn nbsp_here(c: &[char], i: usize) -> bool {
    let p = if i > 0 { c[i - 1] } else { '\0' };
    let n1 = at(c, i + 1);
    let n2 = at(c, i + 2);
    let wb = word_before(c, i);
    if !wb.is_empty() && SHORT.contains(&wb.as_str()) {
        return true;
    }
    let wa = word_after(c, i);
    if ENCLITIC.contains(&wa.as_str()) {
        return true;
    }
    if p == '№' || p == '§' {
        return true;
    }
    // «стр. 15», «рис. 3», «т. 2»
    if p == '.' && n1.is_ascii_digit() && (1..=4).contains(&word_before(c, i - 1).chars().count()) {
        return true;
    }
    if p.is_ascii_digit() {
        if "%‰°€$₽".contains(n1) {
            return true;
        }
        // «1812 г.», «2500 км», но не «12 и»
        if !wa.is_empty() && wa.chars().count() <= 4 && !SHORT.contains(&wa.as_str()) {
            return true;
        }
    }
    // Инициалы: «А. С. Пушкин», «т. е.»
    if i >= 1 {
        if let Some(l) = single_letter_dot(c, i - 1) {
            if l.is_uppercase() && n1.is_uppercase() {
                return true;
            }
            // «т. е.», «Т. е.»
            if n1.is_lowercase() && n2 == '.' {
                return true;
            }
        }
    }
    // «Пушкин А. С.», «Пушкин А.С.»
    let n3 = at(c, i + 3);
    let initial_follows = n3.is_uppercase() && at(c, i + 4) == '.';
    n1.is_uppercase() && n2 == '.' && (!n3.is_alphabetic() || initial_follows) && p.is_alphabetic()
}

/// «10 000»: слева 1–3 цифры, справа ровно 3.
fn thousands(c: &[char], i: usize) -> bool {
    let left = c[..i].iter().rev().take_while(|x| x.is_ascii_digit()).count();
    let right = c[i + 1..].iter().take_while(|x| x.is_ascii_digit()).count();
    (1..=3).contains(&left) && right == 3
}

/// Вставить неразрывный пробел после символа i: «№12», «А.С.», «т.е.».
fn needs_insert(c: &[char], i: usize) -> bool {
    let n1 = at(c, i + 1);
    let n2 = at(c, i + 2);
    if (c[i] == '№' || c[i] == '§') && n1.is_ascii_digit() {
        return true;
    }
    match single_letter_dot(c, i) {
        Some(_) if n1.is_lowercase() && n2 == '.' => true,
        Some(l) => l.is_uppercase() && n1.is_uppercase(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str, lang: Lang) -> String {
        let mut sp = vec![Span::plain(s)];
        apply(&mut sp, lang);
        sp.iter().map(|s| s.text.as_str()).collect::<String>().replace(NBSP, "~")
    }

    #[test]
    fn ellipsis() {
        assert_eq!(t("Он помолчал...", Lang::Ru), "Он помолчал…");
        assert_eq!(t("Что?.. Да!..", Lang::Ru), "Что?.. Да!..");
        assert_eq!(t("Ждал…… и ждал....", Lang::Ru), "Ждал… и~ждал…");
    }

    #[test]
    fn dash_glued_to_punctuation() {
        assert_eq!(t("инженера,— отроду", Lang::Ru), "инженера,~— отроду");
        assert_eq!(t("сказал,- и ушёл", Lang::Ru), "сказал,~— и~ушёл");
        assert_eq!(t("1941—1945", Lang::Ru), "1941—1945");
        assert_eq!(t("Само–реализация и 1914–1918", Lang::Ru), "Само-реализация и~1914–1918");
    }

    #[test]
    fn quotes_ru() {
        assert_eq!(
            t("Он сказал: \"Это слово \"свобода\" ничего\".", Lang::Ru),
            "Он сказал: «Это слово „свобода“ ничего»."
        );
        assert_eq!(t("\"Привет!\" - крикнул он.", Lang::Ru), "«Привет!»~— крикнул он.");
    }

    #[test]
    fn dialogue() {
        assert_eq!(t("- Ты куда? - спросила она.", Lang::Ru), "—~Ты куда?~— спросила она.");
        assert_eq!(t("-- Никуда, -- ответил он.", Lang::Ru), "—~Никуда,~— ответил он.");
    }

    #[test]
    fn ranges_and_phones() {
        assert_eq!(t("Годы 1812-1814, телефон 123-45-67", Lang::Ru), "Годы 1812–1814, телефон 123-45-67");
    }

    #[test]
    fn short_words_and_initials() {
        assert_eq!(t("Он шёл в лес и в поле", Lang::Ru), "Он шёл в~лес и~в~поле");
        assert_eq!(t("А.С. Пушкин", Lang::Ru), "А.~С.~Пушкин");
        assert_eq!(t("т.е. всё", Lang::Ru), "т.~е. всё");
        assert_eq!(t("Т.е. всё", Lang::Ru), "Т.~е. всё");
        assert_eq!(t("№12 и 2500 км", Lang::Ru), "№~12 и~2500~км");
        assert_eq!(t("1 000 000 руб.", Lang::Ru), "1\u{202F}000\u{202F}000~руб.");
        assert_eq!(t("рост 5' 10\"", Lang::Ru), "рост 5′ 10″");
        assert_eq!(t("см. стр. 15", Lang::Ru), "см. стр.~15");
    }

    #[test]
    fn english() {
        assert_eq!(
            t("\"Well,\" he said, \"I don't -- it's the '90s.\"", Lang::En),
            "“Well,” he said, “I don’t—it’s the ’90s.”"
        );
        assert_eq!(t("rock 'n' roll", Lang::En), "rock ’n’ roll");
    }
}
