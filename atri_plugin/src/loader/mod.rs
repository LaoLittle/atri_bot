use atri_ffi::ffi::AtriVTable;
use std::mem::MaybeUninit;

static mut ATRI_VTABLE: MaybeUninit<&'static AtriVTable> = MaybeUninit::uninit();

#[no_mangle]
extern "C" fn plugin_init(vtb: *const AtriVTable) {
    unsafe {
        let vtb: &'static _ = &*vtb;
        ATRI_VTABLE = MaybeUninit::new(vtb);
    }
}

pub fn get_plugin_vtable() -> &'static AtriVTable {
    unsafe { ATRI_VTABLE.assume_init() }
}
