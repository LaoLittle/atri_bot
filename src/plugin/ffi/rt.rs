use super::cast_ref;
use crate::service::plugin::PluginManager;
use atri_ffi::error::FFIResult;
use atri_ffi::future::FFIFuture;
use atri_ffi::{Managed, RustString};
use std::ffi::{c_char, CStr};
use std::future::Future;

pub extern "C" fn plugin_manager_spawn(
    manager: *const (),
    future: FFIFuture<Managed>,
) -> FFIFuture<FFIResult<Managed>> {
    let manager: &PluginManager = cast_ref(manager);
    let handle = manager.async_runtime().spawn(future);

    FFIFuture::from(async { FFIResult::from(handle.await) })
}

pub extern "C" fn plugin_manager_block_on(
    manager: *const (),
    future: FFIFuture<Managed>,
) -> Managed {
    let manager: &PluginManager = cast_ref(manager);
    manager.async_runtime().block_on(future)
}

pub fn future_block_on<F>(manager: *const (), future: F) -> F::Output
where
    F: Future,
    F: Send + 'static,
    F::Output: Send + 'static,
{
    let manager: &PluginManager = cast_ref(manager);

    let (tx, rx) = std::sync::mpsc::channel();

    manager.async_runtime().spawn(async move {
        let val = future.await;
        let _ = tx.send(val);
    });

    let rx = || rx.recv().expect("Cannot recv");
    // calling this outside a runtime normally calls the provided closure.
    // all runtime is multi-threaded
    tokio::task::block_in_place(rx)
}
