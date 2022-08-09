use std::mem::MaybeUninit;
use atri_ffi::ffi::{AtriVTable};

static mut ATRI_VTABLE: MaybeUninit<&'static AtriVTable> = MaybeUninit::uninit();

#[no_mangle]
extern fn plugin_init(vtb: *const AtriVTable) {
    unsafe {
        let vtb: &'static _ = &*vtb;
        ATRI_VTABLE = MaybeUninit::new(vtb);
    }
}

pub fn get_plugin_vtable() -> &'static AtriVTable {
    unsafe { ATRI_VTABLE.assume_init() }
}