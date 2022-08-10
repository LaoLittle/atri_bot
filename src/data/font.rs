use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use skia_safe::{Data, Typeface};

pub static FONT_DATA_PATH: &str = "font";

pub fn font_data_buf() -> PathBuf {
    let mut buf = PathBuf::new();
    buf.push(super::DATA_PATH);
    buf.push(FONT_DATA_PATH);
    buf
}

static FONTS: OnceLock<RwLock<HashMap<String, Typeface>>> = OnceLock::new();

pub fn get_dir_font(name: &String) -> Option<Typeface> {
    let fonts = FONTS.get_or_init(|| {
        fs::create_dir_all(font_data_buf()).expect("Cannot create font dir");
        Default::default()
    });

    {
        let read = fonts.read().expect("Cannot read fonts");
        if let Some(f) = read.get(name) {
            return Some(f.clone());
        }
    }

    let mut file = font_data_buf();
    file.push(name);

    let bytes = fs::read(file).ok()?;
    let data = Data::new_copy(&bytes);
    Typeface::from_data(data, 0)
}
