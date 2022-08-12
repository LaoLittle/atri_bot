use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::{FFIFuture};
use atri_ffi::Managed;
use crate::{Event, Listener};

pub extern "C" fn new_listener(f: FFIFn<FFIFuture<bool>, FFIEvent>) -> Managed {
    let guard = Listener::listening_on(move |e: Event| {
        f.invoke(e.into_ffi())
    })
        .start();

    Managed::from_value(guard)
}