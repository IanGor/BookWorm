//! Терпимый к ошибкам разбор XML/XHTML. Книги часто сломаны: незакрытые теги,
//! голые `&`, атрибуты без кавычек, мусор перед декларацией. Парсер не падает,
//! а восстанавливает дерево как может.
//!
//! Имена элементов и атрибутов приводятся к нижнему регистру и теряют префикс
//! пространства имён: `l:href`, `xlink:href` → `href`, `xml:lang` → `lang`.

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// Первый потомок с таким именем на любой глубине.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for e in self.elements() {
            if e.name == name {
                return Some(e);
            }
            if let Some(f) = e.find(name) {
                return Some(f);
            }
        }
        None
    }

    /// Весь текст элемента и его потомков.
    pub fn text(&self) -> String {
        let mut s = String::new();
        collect_text(self, &mut s);
        s
    }
}

fn collect_text(e: &Element, out: &mut String) {
    for n in &e.children {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Element(c) => collect_text(c, out),
        }
    }
}

/// Элементы HTML без закрывающего тега.
const VOID: &[&str] = &["br", "img", "hr", "meta", "link", "input", "col", "area", "base", "wbr"];
/// Элементы, которые не бывают вложены сами в себя: новый `<p>` закрывает незакрытый.
const NO_NEST: &[&str] = &["p", "v", "li", "td", "th", "tr"];

/// Разбирает документ и возвращает искусственный корень `#root`.
pub fn parse(src: &str) -> Element {
    let b = src.as_bytes();
    let mut stack: Vec<Element> = vec![Element { name: "#root".into(), ..Default::default() }];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let end = memchr(b, i, b'<');
            push_text(&mut stack, &decode_entities(&src[i..end]));
            i = end;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            i = find(src, i + 4, "-->").map_or(b.len(), |p| p + 3);
        } else if rest.starts_with("<![CDATA[") {
            let end = find(src, i + 9, "]]>").unwrap_or(b.len());
            push_text(&mut stack, &src[i + 9..end]);
            i = (end + 3).min(b.len());
        } else if rest.starts_with("<!") {
            i = skip_decl(b, i);
        } else if rest.starts_with("<?") {
            i = find(src, i + 2, "?>").map_or(b.len(), |p| p + 2);
        } else if rest.starts_with("</") {
            let end = memchr(b, i, b'>');
            let name = local_name(src[i + 2..end].trim());
            close(&mut stack, &name);
            i = (end + 1).min(b.len());
        } else if b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            i = open_tag(src, i, &mut stack);
        } else {
            push_text(&mut stack, "<");
            i += 1;
        }
    }
    while stack.len() > 1 {
        pop(&mut stack);
    }
    stack.pop().unwrap_or_default()
}

fn memchr(b: &[u8], from: usize, c: u8) -> usize {
    b[from..].iter().position(|&x| x == c).map_or(b.len(), |p| p + from)
}

fn find(src: &str, from: usize, pat: &str) -> Option<usize> {
    src.get(from..)?.find(pat).map(|p| p + from)
}

/// Пропускает `<!DOCTYPE …>` вместе с внутренним подмножеством `[…]`.
fn skip_decl(b: &[u8], start: usize) -> usize {
    let mut depth = 0i32;
    for (k, &c) in b[start..].iter().enumerate() {
        match c {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'>' if depth <= 0 => return start + k + 1,
            _ => {}
        }
    }
    b.len()
}

fn local_name(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).to_ascii_lowercase()
}

fn open_tag(src: &str, start: usize, stack: &mut Vec<Element>) -> usize {
    let b = src.as_bytes();
    let mut i = start + 1;
    let ns = i;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name = local_name(&src[ns..i]);
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
        if b[i] == b'/' {
            if b.get(i + 1) == Some(&b'>') {
                self_closing = true;
            }
            i += 1;
            continue;
        }
        let ks = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        if i == ks {
            // Одиночный `=` или другой мусор: пропускаем, чтобы не зациклиться.
            i += 1;
            continue;
        }
        let key = local_name(&src[ks..i]);
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let vs = i + 1;
                let ve = memchr(b, vs, b[i]);
                value = decode_entities(&src[vs..ve]);
                i = (ve + 1).min(b.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                value = decode_entities(&src[vs..i]);
            }
        }
        attrs.push((key, value));
    }

    if NO_NEST.contains(&name.as_str()) && stack.last().is_some_and(|e| e.name == name) {
        pop(stack);
    }
    let el = Element { name, attrs, children: Vec::new() };
    if self_closing || VOID.contains(&el.name.as_str()) {
        if let Some(top) = stack.last_mut() {
            top.children.push(Node::Element(el));
        }
    } else {
        stack.push(el);
    }
    i
}

/// Закрывает элемент с этим именем и всё незакрытое внутри него.
/// Закрывающий тег без пары игнорируется.
fn close(stack: &mut Vec<Element>, name: &str) {
    if let Some(pos) = stack.iter().rposition(|e| e.name == name) {
        if pos == 0 {
            return;
        }
        while stack.len() > pos {
            pop(stack);
        }
    }
}

fn pop(stack: &mut Vec<Element>) {
    if let Some(e) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(Node::Element(e));
        }
    }
}

fn push_text(stack: &mut [Element], text: &str) {
    if text.is_empty() {
        return;
    }
    let Some(top) = stack.last_mut() else { return };
    if let Some(Node::Text(t)) = top.children.last_mut() {
        t.push_str(text);
    } else {
        top.children.push(Node::Text(text.to_string()));
    }
}

/// Раскрывает сущности XML и распространённые сущности HTML.
/// Неизвестные и сломанные (`Smith & Wesson`) оставляет как есть.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        let semi = tail.as_bytes().iter().take(12).position(|&c| c == b';');
        if let Some(semi) = semi {
            if let Some(c) = entity(&tail[1..semi]) {
                out.push(c);
                rest = &tail[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        "shy" => '\u{00AD}',
        "mdash" => '—',
        "ndash" => '–',
        "minus" => '−',
        "hellip" => '…',
        "laquo" => '«',
        "raquo" => '»',
        "bdquo" => '„',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        "sbquo" => '‚',
        "thinsp" => '\u{2009}',
        "ensp" => '\u{2002}',
        "emsp" => '\u{2003}',
        "zwsp" => '\u{200B}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "deg" => '°',
        "times" => '×',
        "divide" => '÷',
        "plusmn" => '±',
        "middot" => '·',
        "bull" => '•',
        "sect" => '§',
        "para" => '¶',
        "euro" => '€',
        "pound" => '£',
        "frac12" => '½',
        "frac14" => '¼',
        "frac34" => '¾',
        "sup2" => '²',
        "sup3" => '³',
        "prime" => '′',
        "Prime" => '″',
        "dagger" => '†',
        "Dagger" => '‡',
        "acute" => '´',
        "iexcl" => '¡',
        "iquest" => '¿',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_markup() {
        let root = parse("\u{FEFF}  <?xml version=\"1.0\"?><a><p>раз & два<p>три</p><b x=#n1>t</b></a>");
        let a = root.child("a").unwrap();
        let ps: Vec<String> = a.elements().filter(|e| e.name == "p").map(|p| p.text()).collect();
        assert_eq!(ps, vec!["раз & два", "три"]);
        assert_eq!(a.child("b").unwrap().attr("x"), Some("#n1"));
    }

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a&nbsp;b&mdash;&#1078;&#x436;&unknown;"), "a\u{00A0}b—жж&unknown;");
    }
}
