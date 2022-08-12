use atri_ffi::ffi::{AtriManager, AtriVTable};
use std::mem::MaybeUninit;

static mut ATRI_VTABLE: MaybeUninit<AtriManager> = MaybeUninit::uninit();

#[no_mangle]
extern "C" fn atri_manager_init(vtb: AtriManager) {
    unsafe {
        ATRI_VTABLE = MaybeUninit::new(vtb);
    }
}

pub(crate) fn get_plugin_manager() -> *const () {
    unsafe { ATRI_VTABLE.assume_init_ref().manager_ptr }
}

pub(crate) fn get_plugin_manager_vtb() -> &'static AtriVTable {
    unsafe { &*ATRI_VTABLE.assume_init_ref().vtb }
}
