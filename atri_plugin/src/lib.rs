use atri_ffi::plugin::{PluginVTable};
use atri_ffi::Managed;

pub mod loader;
pub mod manager;
pub mod listener;

pub use atri_ffi::plugin::PluginInstance;

pub trait Plugin: Sized {
    fn enable(&mut self);

    fn disable(&mut self) {
        // nop
    }

    fn into_instance(self) -> PluginInstance {
        let instance = Managed::from_value(self);
        let vtb = PluginVTable::from(__enable::<Self>, __disable::<Self>);

        PluginInstance::from(instance, vtb)
    }
}

extern fn __enable<P: Plugin>(ptr: *mut ()) {
    // Safety: Plugin is pinned by box
    let p = unsafe { &mut *(ptr as *mut P) };
    p.enable();
}

extern fn __disable<P: Plugin>(ptr: *mut ()) {
    // Safety: Plugin is pinned by box
    let p = unsafe { &mut *(ptr as *mut P) };
    p.disable();
}
