mod bot;
mod listener;

use std::sync::OnceLock;

use crate::plugin::ffi::listener::new_listener;
use crate::PluginManager;
use atri_ffi::ffi::AtriVTable;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

static PLUGIN_VTABLE: OnceLock<AtriVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const AtriVTable {
    PLUGIN_VTABLE.get_or_init(|| AtriVTable {
        plugin_manager_spawn,
        plugin_manager_block_on,
        new_listener,
    })
}

extern "C" fn plugin_manager_spawn(
    manager: *const (),
    future: FFIFuture<Managed>,
) -> FFIFuture<Managed> {
    let manager = unsafe { &*(manager as *const PluginManager) };
    let handle = manager.async_runtime().spawn(future);

    FFIFuture::from(async { handle.await.unwrap() })
}

extern "C" fn plugin_manager_block_on(manager: *const (), future: FFIFuture<Managed>) -> Managed {
    let manager = unsafe { &*(manager as *const PluginManager) };
    manager.async_runtime().block_on(future)
}
