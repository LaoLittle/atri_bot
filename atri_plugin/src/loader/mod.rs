use atri_ffi::ffi::{AtriManager, AtriVTable};
use std::mem::MaybeUninit;

static mut ATRI_MANAGER: MaybeUninit<AtriManager> = MaybeUninit::uninit();

/// Safety: This function will be called by the plugin manager once
#[no_mangle]
unsafe extern "C" fn atri_manager_init(manager: AtriManager) {
    ATRI_MANAGER = MaybeUninit::new(manager);
}

pub(crate) fn get_plugin_manager() -> *const () {
    unsafe { ATRI_MANAGER.assume_init_ref().manager_ptr }
}

pub(crate) fn get_plugin_manager_vtb() -> &'static AtriVTable {
    unsafe { &*ATRI_MANAGER.assume_init_ref().vtb }
}
