//! Минимальное чтение zip: только то, что нужно для `.fb2.zip` (и позже EPUB) —
//! центральный каталог, методы «без сжатия» и deflate.

pub struct Entry {
    pub name: String,
    method: u16,
    comp_size: usize,
    local: usize,
}

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

/// Записи архива по центральному каталогу.
pub fn entries(b: &[u8]) -> Result<Vec<Entry>, String> {
    const BAD: &str = "повреждённый zip";
    let from = b.len().saturating_sub(22 + 65_535);
    let eocd = (from..b.len().saturating_sub(21))
        .rev()
        .find(|&i| b[i..].starts_with(b"PK\x05\x06"))
        .ok_or(BAD)?;
    let count = u16_at(b, eocd + 10).ok_or(BAD)? as usize;
    let mut p = u32_at(b, eocd + 16).ok_or(BAD)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if !b.get(p..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
            return Err(BAD.into());
        }
        let n = u16_at(b, p + 28).ok_or(BAD)? as usize;
        let extra = u16_at(b, p + 30).ok_or(BAD)? as usize;
        let comment = u16_at(b, p + 32).ok_or(BAD)? as usize;
        let name = b.get(p + 46..p + 46 + n).ok_or(BAD)?;
        out.push(Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            method: u16_at(b, p + 10).ok_or(BAD)?,
            comp_size: u32_at(b, p + 20).ok_or(BAD)?,
            local: u32_at(b, p + 42).ok_or(BAD)?,
        });
        p += 46 + n + extra + comment;
    }
    Ok(out)
}

pub fn read(b: &[u8], e: &Entry) -> Result<Vec<u8>, String> {
    let bad = || format!("{}: повреждённая запись zip", e.name);
    if !b.get(e.local..).is_some_and(|s| s.starts_with(b"PK\x03\x04")) {
        return Err(bad());
    }
    let n = u16_at(b, e.local + 26).ok_or_else(bad)? as usize;
    let extra = u16_at(b, e.local + 28).ok_or_else(bad)? as usize;
    let start = e.local + 30 + n + extra;
    let data = b.get(start..start + e.comp_size).ok_or_else(bad)?;
    match e.method {
        0 => Ok(data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| bad()),
        m => Err(format!("{}: метод сжатия {m} не поддерживается", e.name)),
    }
}
