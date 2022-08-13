use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub extern "C" fn event_intercept(intercepted: *const ()) {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.swap(true, Ordering::Release);
}

pub extern "C" fn event_is_intercepted(intercepted: *const ()) -> bool {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.load(Ordering::Relaxed)
}
