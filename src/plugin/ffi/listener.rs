use crate::event::listener::{ListenerBuilder, Priority};
use crate::{Event, Listener};
use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::FFIFuture;
use atri_ffi::{FFIOption, Managed};
use futures::FutureExt;
use std::sync::Arc;
use std::time::Duration;

pub extern "C" fn new_listener(
    concurrent: bool,
    f: FFIFn<FFIEvent, FFIFuture<bool>>,
    priority: u8,
) -> Managed {
    let guard = ListenerBuilder::listening_on(move |e: Event| f.invoke(e.into_ffi()))
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

pub extern "C" fn new_listener_closure(
    concurrent: bool,
    f: FFIFn<FFIEvent, bool>,
    priority: u8,
) -> Managed {
    let arc = Arc::new(f);
    let guard = ListenerBuilder::listening_on(move |e: Event| {
        let f = Arc::clone(&arc);
        tokio::task::spawn_blocking(move || f.invoke(e.into_ffi())).map(Result::unwrap)
    })
    .concurrent(concurrent)
    .priority(Priority::from(priority))
    .start();

    Managed::from_value(guard)
}

pub extern "C" fn new_listener_c_func(
    concurrent: bool,
    f: extern "C" fn(FFIEvent) -> bool,
    priority: u8,
) -> Managed {
    let guard = ListenerBuilder::listening_on(move |e: Event| {
        tokio::task::spawn_blocking(move || f(e.into_ffi())).map(Result::unwrap)
    })
    .concurrent(concurrent)
    .priority(Priority::from(priority))
    .start();

    Managed::from_value(guard)
}
