//! Ядро BookWorm: разбор книг, типограф, вёрстка абзацев и разбиение на страницы.
//! Ядро не знает об Android: на вход — байты книги и файлы шрифтов,
//! на выход — страницы со списком глифов и их координатами.

pub mod book;
pub mod css;
pub mod epub;
pub mod fb2;
pub mod font;
pub mod hyph;
pub mod layout;
pub mod model;
pub mod para;
#[cfg(feature = "raster")]
pub mod render;
pub mod typo;
pub mod xml;
pub mod zip;
