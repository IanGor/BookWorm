//! Подмножество CSS книги (SPEC §2.2). Берём только то, что влияет на смысл
//! текста: курсив, жирный, индексы, выравнивание по центру и вправо, отступы
//! в em и процентах, `display: none`, разрывы страниц. Шрифты, абсолютные
//! размеры, цвета и интерлиньяж издателя игнорируются — вёрстка своя.
//!
//! Селекторы: тег, `.класс`, `#id`, их сочетания и потомки через пробел
//! (`>` считается потомком). Селекторы с `+`, `~`, `[атрибутами]` и `:псевдо`
//! пропускаются целиком: применённые неточно, они испортили бы больше, чем дали.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
struct Compound {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
}

#[derive(Debug, Clone)]
struct Rule {
    /// Цепочка потомков; последний — сам элемент.
    parts: Vec<Compound>,
    spec: u32,
    order: usize,
    decls: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone)]
pub struct Sheet {
    rules: Vec<Rule>,
}

/// Элемент для сопоставления с селекторами.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

impl Sheet {
    pub fn parse(src: &str) -> Sheet {
        let mut sheet = Sheet::default();
        sheet.add(src);
        sheet
    }

    /// Добавляет правила (следующий файл стилей или `<style>`): они идут позже.
    pub fn add(&mut self, src: &str) {
        let src = strip_comments(src);
        let cs: Vec<char> = src.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            if cs[i].is_whitespace() || cs[i] == '}' || cs[i] == ';' {
                i += 1;
                continue;
            }
            if cs[i] == '@' {
                // @media, @font-face, @page — пропускаем вместе с блоком; @import — до «;».
                let semi = cs[i..].iter().position(|&c| c == ';');
                let brace = cs[i..].iter().position(|&c| c == '{');
                match (semi, brace) {
                    (Some(s), Some(b)) if s < b => i += s + 1,
                    (_, Some(b)) => i = skip_block(&cs, i + b),
                    (Some(s), None) => i += s + 1,
                    (None, None) => break,
                }
                continue;
            }
            let Some(open) = cs[i..].iter().position(|&c| c == '{').map(|p| p + i) else { break };
            let close = cs[open..].iter().position(|&c| c == '}').map_or(cs.len(), |p| p + open);
            let selectors: String = cs[i..open].iter().collect();
            let body: String = cs[open + 1..close].iter().collect();
            let decls = declarations(&body);
            for sel in selectors.split(',') {
                if let Some((parts, spec)) = selector(sel) {
                    let order = self.rules.len();
                    self.rules.push(Rule { parts, spec, order, decls: decls.clone() });
                }
            }
            i = close + 1;
        }
    }

    /// Объявления для элемента `chain.last()` (предки — раньше в `chain`) по каскаду:
    /// специфичность, затем порядок; встроенный `style` — последним.
    pub fn computed(&self, chain: &[Info], inline: Option<&str>) -> HashMap<String, String> {
        let mut hits: Vec<&Rule> = self.rules.iter().filter(|r| matches(&r.parts, chain)).collect();
        hits.sort_by_key(|r| (r.spec, r.order));
        let mut out = HashMap::new();
        for r in hits {
            for (k, v) in &r.decls {
                out.insert(k.clone(), v.clone());
            }
        }
        if let Some(s) = inline {
            for (k, v) in declarations(s) {
                out.insert(k, v);
            }
        }
        out
    }
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find("/*") {
        out.push_str(&rest[..p]);
        rest = rest[p + 2..].find("*/").map_or("", |e| &rest[p + 2 + e + 2..]);
    }
    out.push_str(rest);
    out
}

/// Пропускает блок `{…}` с вложенными скобками; `i` — позиция `{`.
fn skip_block(cs: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < cs.len() {
        match cs[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn declarations(body: &str) -> Vec<(String, String)> {
    body.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let v = v.trim().trim_end_matches("!important").trim().to_ascii_lowercase();
            Some((k.trim().to_ascii_lowercase(), v))
        })
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

fn selector(s: &str) -> Option<(Vec<Compound>, u32)> {
    let s = s.trim().replace('>', " ");
    if s.is_empty() || s.contains(['+', '~', '[', ':', '*']) {
        return None;
    }
    let mut parts = Vec::new();
    let mut spec = 0;
    for tok in s.split_whitespace() {
        let mut c = Compound::default();
        let mut cur = String::new();
        let mut kind = 't';
        for ch in tok.chars().chain(std::iter::once('.')) {
            if ch != '.' && ch != '#' {
                cur.push(ch);
                continue;
            }
            if !cur.is_empty() {
                let v = std::mem::take(&mut cur).to_ascii_lowercase();
                match kind {
                    '.' => {
                        c.classes.push(v);
                        spec += 10;
                    }
                    '#' => {
                        c.id = Some(v);
                        spec += 100;
                    }
                    _ => {
                        c.tag = Some(v);
                        spec += 1;
                    }
                }
            }
            kind = ch;
        }
        parts.push(c);
    }
    Some((parts, spec))
}

fn compound_matches(c: &Compound, e: &Info) -> bool {
    c.tag.as_ref().is_none_or(|t| *t == e.tag)
        && c.id.as_ref().is_none_or(|id| e.id.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(id)))
        && c.classes.iter().all(|k| e.classes.iter().any(|x| x.eq_ignore_ascii_case(k)))
}

fn matches(parts: &[Compound], chain: &[Info]) -> bool {
    let (Some(last), Some(me)) = (parts.last(), chain.last()) else { return false };
    if !compound_matches(last, me) {
        return false;
    }
    // Предки: каждый следующий компонент ищем выше предыдущего найденного.
    let mut j = chain.len() - 1;
    for p in parts[..parts.len() - 1].iter().rev() {
        loop {
            if j == 0 {
                return false;
            }
            j -= 1;
            if compound_matches(p, &chain[j]) {
                break;
            }
        }
    }
    true
}

/// Длина в em или процентах ширины; абсолютные единицы не берём.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Len {
    pub em: f32,
    pub pct: f32,
}

pub fn length(v: &str) -> Option<Len> {
    let v = v.trim();
    let num = |s: &str| s.trim().parse::<f32>().ok();
    if v == "0" {
        return Some(Len::default());
    }
    if let Some(n) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")).and_then(num) {
        return Some(Len { em: n, pct: 0.0 });
    }
    if let Some(n) = v.strip_suffix('%').and_then(num) {
        return Some(Len { em: 0.0, pct: n / 100.0 });
    }
    None
}

/// Левое и правое поле из `margin-left/right` и сокращения `margin`.
pub fn margins(d: &HashMap<String, String>) -> (Len, Len) {
    let mut left = Len::default();
    let mut right = Len::default();
    if let Some(m) = d.get("margin") {
        let v: Vec<&str> = m.split_whitespace().collect();
        let (r, l) = match v.len() {
            1 => (v[0], v[0]),
            2 | 3 => (v[1], v[1]),
            4 => (v[1], v[3]),
            _ => ("0", "0"),
        };
        left = length(l).unwrap_or_default();
        right = length(r).unwrap_or_default();
    }
    if let Some(l) = d.get("margin-left").and_then(|v| length(v)) {
        left = l;
    }
    if let Some(r) = d.get("margin-right").and_then(|v| length(v)) {
        right = r;
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(tag: &str, classes: &[&str]) -> Info {
        Info { tag: tag.into(), classes: classes.iter().map(|s| s.to_string()).collect(), id: None }
    }

    #[test]
    fn cascade() {
        let s = Sheet::parse(
            "@font-face { font-family: X; src: url(a.otf); }\n/* комментарий */\n\
             p { text-indent: 1.5em; margin: 0 }\n.center { text-align: center; text-indent: 0 }\n\
             h1 + p { text-indent: 0 }\n.poem p.line { text-indent: -1em; margin-left: 1em }\n\
             @media amzn-kf8 { p { color: red } }\n.hidden { display: none !important }",
        );
        let p = s.computed(&[info("body", &[]), info("p", &[])], None);
        assert_eq!(p["text-indent"], "1.5em");
        assert!(!p.contains_key("color"));
        let c = s.computed(&[info("p", &["center"])], None);
        assert_eq!(c["text-indent"], "0");
        assert_eq!(c["text-align"], "center");
        let line = s.computed(&[info("div", &["poem"]), info("div", &["stanza"]), info("p", &["line"])], None);
        assert_eq!(line["text-indent"], "-1em");
        assert_eq!(margins(&line).0, Len { em: 1.0, pct: 0.0 });
        let lone = s.computed(&[info("p", &["line"])], None);
        assert_eq!(lone["text-indent"], "1.5em");
        assert_eq!(s.computed(&[info("p", &["hidden"])], Some("font-style: italic"))["display"], "none");
        assert_eq!(s.computed(&[info("p", &[])], Some("font-style: italic"))["font-style"], "italic");
        assert_eq!(length("40%"), Some(Len { em: 0.0, pct: 0.4 }));
        assert_eq!(length("12pt"), None);
    }
}
