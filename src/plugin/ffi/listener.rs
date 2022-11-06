use crate::event::listener::Priority;
use crate::{Event, Listener};
use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use std::time::Duration;

pub extern "C" fn new_listener(f: FFIFn<FFIEvent, FFIFuture<bool>>) -> Managed {
    let guard = Listener::listening_on(move |e: Event| f.invoke(e.into_ffi())).start();

    Managed::from_value(guard)
}

pub extern "C" fn listener_next_event_with_priority(
    millis: u64,
    filter: FFIFn<FFIEvent, bool>,
    priority: u8,
) {
    let fu = FFIFuture::from(async move {
        let option = Listener::next_event_with_priority(
            Duration::from_millis(millis),
            move |e: &Event| {
                let ffi = e.clone().into_ffi();

                filter.invoke(ffi)
            },
            Priority::from(priority),
        )
        .await;
    });
}
