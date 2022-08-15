use std::sync::atomic::{AtomicBool, Ordering};
use atri_ffi::Managed;
use crate::event::GroupMessageEvent;
use crate::plugin::cast_ref;

pub extern "C" fn event_intercept(intercepted: *const ()) {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.swap(true, Ordering::Release);
}

pub extern "C" fn event_is_intercepted(intercepted: *const ()) -> bool {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.load(Ordering::Relaxed)
}

pub extern "C" fn group_message_event_get_bot(event: *const ()) -> Managed {
    let event: &GroupMessageEvent = cast_ref(event);
    Managed::from_value(event.bot().clone())
}

pub extern "C" fn group_message_event_get_group(event: *const ()) -> Managed {
    let event: &GroupMessageEvent = cast_ref(event);
    Managed::from_value(event.group().clone())
}

pub extern "C" fn group_message_event_get_message(event: *const ()) {

}