use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, RwLock};

pub static TERMINAL_CLOSED: AtomicBool = AtomicBool::new(false);
pub static INPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

pub struct InputCache {
    pub caches: Vec<String>,
    pub index: usize,
    pub last_input: String,
}

pub static INPUT_CACHE: Mutex<InputCache> = Mutex::new(InputCache {
    caches: vec![],
    index: 0,
    last_input: String::new(),
});

pub static IS_MAIN: AtomicBool = AtomicBool::new(true);

pub static PLAYBACK_BUFFER: Mutex<Vec<Vec<u8>>> = Mutex::new(vec![]);
