use std::sync::OnceLock;

use atri_ffi::ffi::AtriVTable;

static PLUGIN_VTABLE: OnceLock<AtriVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const AtriVTable {
    PLUGIN_VTABLE.get_or_init(|| {
        AtriVTable {}
    })
}