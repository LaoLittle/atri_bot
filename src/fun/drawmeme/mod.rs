use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use bytes::Bytes;
use skia_safe::Bitmap;

pub mod zero;

pub trait Meme {
    fn draw(args: &[MemeArg]) -> Bitmap;
}

pub enum MemeArg {
    Text(String),
    Image(Bytes),
}

pub enum MemeError {
    Other(String)
}