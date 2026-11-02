//! Мост JNI: приложение на Kotlin (класс `app.bookworm.Core`) вызывает ядро.
//! Книга живёт в Rust; Kotlin держит только её номер (указатель) и получает
//! готовые пиксели страниц. Ошибки и паники не роняют приложение: функция
//! возвращает значение по умолчанию, а текст ошибки отдаёт `lastError()`.

use bookworm_core::book::Book;
use bookworm_core::font::{families, FaceSpec, FontSet};
use bookworm_core::layout::{Geometry, DEFAULT_STEP, SIZE_STEPS};
use bookworm_core::model::Position;
use bookworm_core::render;
use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard};

static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn set_error(msg: String) {
    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = msg;
}

fn guard<T>(default: T, f: impl FnOnce() -> Result<T, String>) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            set_error(e);
            default
        }
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "неизвестная ошибка".into());
            set_error(format!("сбой ядра: {msg}"));
            default
        }
    }
}

fn book<'a>(h: jlong) -> Result<MutexGuard<'a, Book>, String> {
    if h == 0 {
        return Err("книга не открыта".into());
    }
    // SAFETY: `h` получен из `open` и ещё не передан в `close` (это гарантирует Kotlin).
    let m = unsafe { &*(h as *const Mutex<Book>) };
    Ok(m.lock().unwrap_or_else(|e| e.into_inner()))
}

fn rust_str(env: &mut JNIEnv, s: &JString) -> Result<String, String> {
    env.get_string(s).map(Into::into).map_err(|e| e.to_string())
}

fn java_str(env: &mut JNIEnv, s: Option<String>) -> jstring {
    match s {
        Some(s) => env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

fn copy_to(env: &mut JNIEnv, buf: &JByteBuffer, data: &[u8]) -> Result<(), String> {
    let cap = env.get_direct_buffer_capacity(buf).map_err(|e| e.to_string())?;
    if cap < data.len() {
        return Err(format!("буфер мал: {cap} < {}", data.len()));
    }
    let ptr = env.get_direct_buffer_address(buf).map_err(|e| e.to_string())?;
    // SAFETY: буфер прямой и не короче `data` (проверено выше).
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
    Ok(())
}

fn theme(night: jboolean) -> render::Theme {
    if night == JNI_TRUE {
        render::NIGHT
    } else {
        render::DAY
    }
}

fn pack(p: Position) -> jlong {
    ((p.block as i64) << 32) | p.offset as i64
}

fn unpack(v: jlong) -> Position {
    Position { block: (v >> 32) as u32, offset: v as u32 }
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_lastError<'l>(mut env: JNIEnv<'l>, _: JObject<'l>) -> jstring {
    let msg = LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    java_str(&mut env, Some(msg))
}

/// Открывает и верстает книгу. 0 — ошибка (см. lastError).
/// `fonts` — четыре строки `путь|насыщенность` (обычный, курсив, жирный, жирный курсив),
/// `step` — ступень размера (кегль опорного шрифта при ширине 1080), `scroll` — прокрутка,
/// `bionic` — выделение начальной трети слов.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_app_bookworm_Core_open<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    path: JString<'l>,
    fonts: JString<'l>,
    mono: JString<'l>,
    width: jint,
    height: jint,
    step: f32,
    scroll: jboolean,
    bionic: jboolean,
) -> jlong {
    guard(0, || {
        let path = rust_str(&mut env, &path)?;
        let fonts = rust_str(&mut env, &fonts)?;
        let mono = rust_str(&mut env, &mono)?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let specs: Vec<FaceSpec> = fonts.lines().filter_map(FaceSpec::parse).collect();
        let specs: [FaceSpec; 4] = specs.try_into().map_err(|_| "нужно четыре начертания шрифта")?;
        let fonts = FontSet::from_specs(&specs, &mono)?;
        let mut g = Geometry::new(width as f32, height as f32, step, fonts.faces[0].x_height, scroll == JNI_TRUE);
        g.bionic = bionic == JNI_TRUE;
        let book = Book::open(&bytes, fonts, g)?;
        Ok(Box::into_raw(Box::new(Mutex::new(book))) as jlong)
    })
}

/// Гарнитуры из файлов (пути через перевод строки): строки
/// `название<TAB>кириллица 0/1<TAB>обычный<TAB>курсив<TAB>жирный<TAB>жирный курсив`.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_fontFamilies<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, paths: JString<'l>) -> jstring {
    let r = guard(None, || {
        let paths: Vec<String> = rust_str(&mut env, &paths)?.lines().map(String::from).collect();
        let mut out = String::new();
        for f in families(&paths) {
            let faces: Vec<String> = f.faces.iter().map(FaceSpec::encode).collect();
            out.push_str(&format!("{}\t{}\t{}\n", f.name.replace('\t', " "), f.cyrillic as u8, faces.join("\t")));
        }
        Ok(Some(out))
    });
    java_str(&mut env, r)
}

/// Ступени размера через пробел и номер ступени по умолчанию последним числом.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_sizeSteps<'l>(mut env: JNIEnv<'l>, _: JObject<'l>) -> jstring {
    let mut s: Vec<String> = SIZE_STEPS.iter().map(|v| v.to_string()).collect();
    s.push(DEFAULT_STEP.to_string());
    java_str(&mut env, Some(s.join(" ")))
}

/// Прокрутка: высота главы (в этом режиме страница = глава).
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_pageHeight<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint) -> f32 {
    guard(0.0, || Ok(book(h)?.page(page.max(0) as usize).map_or(0.0, |p| p.height)))
}

/// Прокрутка: кусок главы высотой `height` px с высоты `top` в буфер RGBA.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_renderStrip<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    h: jlong,
    page: jint,
    top: f32,
    height: jint,
    night: jboolean,
    out: JByteBuffer<'l>,
) -> jboolean {
    guard(JNI_FALSE, || {
        let pix = render::strip(&*book(h)?, page.max(0) as usize, top, height.max(1) as u32, &theme(night));
        copy_to(&mut env, &out, pix.data())?;
        Ok(JNI_TRUE)
    })
}

/// Прокрутка: высота полосы строки статуса (нижнее поле).
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_footerHeight<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong) -> jint {
    guard(0, || Ok(book(h)?.g.margin_y.ceil() as jint))
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_renderFooter<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    h: jlong,
    page: jint,
    top: f32,
    night: jboolean,
    out: JByteBuffer<'l>,
) -> jboolean {
    guard(JNI_FALSE, || {
        let pix = render::footer_band(&*book(h)?, page.max(0) as usize, top, &theme(night));
        copy_to(&mut env, &out, pix.data())?;
        Ok(JNI_TRUE)
    })
}

/// Прокрутка: позиция чтения для окна с верхом на высоте `top`.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_posAt<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint, top: f32) -> jlong {
    guard(0, || Ok(pack(book(h)?.pos_at(page.max(0) as usize, top))))
}

/// Прокрутка: верх окна, при котором позиция стоит под верхним полем.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_topOf<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint, pos: jlong) -> f32 {
    guard(0.0, || Ok(book(h)?.top_of(page.max(0) as usize, unpack(pos))))
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_close<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong) {
    if h != 0 {
        // SAFETY: `h` получен из `open`, Kotlin закрывает книгу ровно один раз.
        drop(unsafe { Box::from_raw(h as *mut Mutex<Book>) });
    }
}

/// Название и авторы из самого файла, без вёрстки: «название<LF>автор1; автор2». Пусто — если нет данных.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_meta<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, path: JString<'l>) -> jstring {
    let r = guard(None, || {
        let path = rust_str(&mut env, &path)?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let doc = bookworm_core::book::parse(&bytes)?;
        let clean = |s: &str| s.replace(['\n', ';'], " ").trim().to_string();
        let authors: Vec<String> = doc.authors.iter().map(|a| clean(a)).filter(|a| !a.is_empty()).collect();
        Ok(Some(format!("{}\n{}", clean(&doc.title), authors.join("; "))))
    });
    java_str(&mut env, r)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_title<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong) -> jstring {
    let t = guard(None, || Ok(Some(book(h)?.doc.title.clone())));
    java_str(&mut env, t)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_blockAt<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint, x: f32, y: f32) -> jint {
    guard(-1, || Ok(book(h)?.hit(page.max(0) as usize, x, y).block.map_or(-1, |b| b as jint)))
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_blockText<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, block: jint) -> jstring {
    let text = guard(None, || Ok(book(h)?.doc.blocks.get(block as usize).map(|b| b.text())));
    java_str(&mut env, text)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_selectionCells<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint, top: f32, height: f32) -> jstring {
    let text = guard(None, || Ok(Some(book(h)?.selection_cells(page.max(0) as usize, top, height))));
    java_str(&mut env, text)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_selectionText<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong) -> jstring {
    let text = guard(None, || Ok(Some(book(h)?.selection_text())));
    java_str(&mut env, text)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_selectionReport<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, start: jint, end: jint) -> jstring {
    let text = guard(None, || Ok(book(h)?.selection_report(start as usize, end as usize)));
    java_str(&mut env, text)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_typographyReport<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, block: jint) -> jstring {
    let text = guard(None, || Ok(book(h)?.typography_report(block as usize)));
    java_str(&mut env, text)
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_pageCount<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong) -> jint {
    guard(0, || Ok(book(h)?.page_count() as jint))
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_render<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    h: jlong,
    page: jint,
    night: jboolean,
    out: JByteBuffer<'l>,
) -> jboolean {
    guard(JNI_FALSE, || {
        let pix = render::page(&*book(h)?, page.max(0) as usize, &theme(night));
        copy_to(&mut env, &out, pix.data())?;
        Ok(JNI_TRUE)
    })
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_pageStart<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint) -> jlong {
    guard(0, || Ok(book(h)?.page(page.max(0) as usize).map_or(0, |p| pack(p.start))))
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_pageOf<'l>(_: JNIEnv<'l>, _: JObject<'l>, h: jlong, pos: jlong) -> jint {
    guard(0, || Ok(book(h)?.page_of(unpack(pos)) as jint))
}

/// id сноски под пальцем или null.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_hitNote<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    h: jlong,
    page: jint,
    x: f32,
    y: f32,
) -> jstring {
    let id = guard(None, || Ok(book(h)?.hit(page.max(0) as usize, x, y).note));
    java_str(&mut env, id)
}

/// Знаки сносок на странице: строки `x0 y0 x1 y1 id` — касание проверяет Kotlin сам,
/// не дожидаясь фонового потока.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_noteAreas<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, page: jint) -> jstring {
    let r = guard(None, || {
        let mut out = String::new();
        for (a, id) in book(h)?.note_areas(page.max(0) as usize) {
            out.push_str(&format!("{} {} {} {} {id}\n", a[0], a[1], a[2], a[3]));
        }
        Ok(Some(out))
    });
    java_str(&mut env, r)
}

/// Оглавление: строки `уровень<TAB>страница<TAB>позиция<TAB>название`.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_toc<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong) -> jstring {
    let r = guard(None, || {
        let b = book(h)?;
        let mut out = String::new();
        for (t, (level, page, title)) in b.doc.toc.iter().zip(b.toc()) {
            let pos = pack(Position { block: t.block as u32, offset: 0 });
            out.push_str(&format!("{level}\t{page}\t{pos}\t{}\n", title.replace(['\t', '\n'], " ")));
        }
        Ok(Some(out))
    });
    java_str(&mut env, r)
}

/// Высота попапа сноски в пикселях; 0 — сноски нет.
#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_noteHeight<'l>(mut env: JNIEnv<'l>, _: JObject<'l>, h: jlong, id: JString<'l>) -> jint {
    guard(0, || {
        let id = rust_str(&mut env, &id)?;
        Ok(book(h)?.note(&id).map_or(0, |(_, height)| height.ceil() as jint))
    })
}

#[no_mangle]
pub extern "system" fn Java_app_bookworm_Core_renderNote<'l>(
    mut env: JNIEnv<'l>,
    _: JObject<'l>,
    h: jlong,
    id: JString<'l>,
    night: jboolean,
    out: JByteBuffer<'l>,
) -> jboolean {
    guard(JNI_FALSE, || {
        let id = rust_str(&mut env, &id)?;
        let pix = render::note(&*book(h)?, &id, &theme(night)).ok_or("сноска не найдена")?;
        copy_to(&mut env, &out, pix.data())?;
        Ok(JNI_TRUE)
    })
}
