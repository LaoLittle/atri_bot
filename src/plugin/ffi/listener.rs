use crate::event::listener::Priority;
use crate::{Event, Listener};
use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::FFIFuture;
use atri_ffi::{FFIOption, Managed};
use std::time::Duration;

pub extern "C" fn new_listener(concurrent: bool,f: FFIFn<FFIEvent, FFIFuture<bool>>, priority: u8) -> Managed {
    let guard = Listener::listening_on(move |e: Event| f.invoke(e.into_ffi()))
        .concurrent(concurrent)
        .priority(Priority::from(priority))
        .start();

    Managed::from_value(guard)
}

pub extern "C" fn listener_next_event_with_priority(
    millis: u64,
    filter: FFIFn<FFIEvent, bool>,
    priority: u8,
) -> FFIFuture<FFIOption<FFIEvent>> {
    FFIFuture::from(async move {
        let option = Listener::next_event_with_priority(
            Duration::from_millis(millis),
            move |e: &Event| {
                let ffi = e.clone().into_ffi();

                filter.invoke(ffi)
            },
            Priority::from(priority),
        )
        .await
        .map(Event::into_ffi);

        FFIOption::from(option)
    })
}
