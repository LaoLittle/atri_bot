use std::mem::MaybeUninit;
use ffi::future::FFIFuture;
use crate::loader::ffi::event::FFIEvent;
use crate::loader::ffi::Managed;

mod ffi;

#[repr(C)]
pub struct PluginVTable {
    pub new_receiver: extern fn() -> Managed,
    pub receiver_receive: extern fn(*mut ()) -> FFIFuture<FFIEvent>,
}

static mut PLUGIN_VTABLE: MaybeUninit<&'static PluginVTable> = MaybeUninit::uninit();

#[no_mangle]
extern fn plugin_init(vtb: *const PluginVTable) {
    unsafe {
        let vtb: &'static _ = &*vtb;
        PLUGIN_VTABLE = MaybeUninit::new(vtb);
    }
}

pub fn get_plugin_vtable() -> &'static PluginVTable {
    unsafe { PLUGIN_VTABLE.assume_init() }
}