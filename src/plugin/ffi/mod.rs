use std::sync::OnceLock;

use crate::get_runtime;
use atri_ffi::ffi::AtriVTable;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

static PLUGIN_VTABLE: OnceLock<AtriVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const AtriVTable {
    PLUGIN_VTABLE.get_or_init(|| AtriVTable {
        plugin_manager_spawn,
        plugin_manager_block_on,
    })
}

extern "C" fn plugin_manager_spawn(future: FFIFuture<Managed>) -> FFIFuture<Managed> {
    let handle = get_runtime().spawn(future);

    FFIFuture::from(async { handle.await.unwrap() })
}

extern "C" fn plugin_manager_block_on(future: FFIFuture<Managed>) -> Managed {
    get_runtime().block_on(future)
}
