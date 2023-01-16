use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

pub static TERMINAL_CLOSED: AtomicBool = AtomicBool::new(false);
pub static INPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

pub fn is_terminal_closed() -> bool {
    TERMINAL_CLOSED.load(Ordering::Relaxed)
}

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

pub static ALTERNATE_SCREEN: AtomicBool = AtomicBool::new(false);

//pub static PLAYBACK_BUFFER: Mutex<Vec<Vec<u8>>> = Mutex::new(vec![]);

pub fn enter_alternate_screen() {
    ALTERNATE_SCREEN.store(true, Ordering::Release);
}

pub fn exit_alternate_screen() {
    ALTERNATE_SCREEN.store(false, Ordering::Release);
}

pub fn is_alternate_screen_enabled() -> bool {
    ALTERNATE_SCREEN.load(Ordering::Relaxed)
}
