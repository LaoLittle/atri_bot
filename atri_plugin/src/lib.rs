use atri_ffi::plugin::{PluginVTable};
use atri_ffi::Managed;

pub mod loader;
pub mod manager;
pub mod listener;
pub mod event;

pub use atri_ffi::plugin::PluginInstance;

pub trait Plugin: Sized {
    /// 插件启用
    fn enable(&mut self);

    /// 插件禁用
    fn disable(&mut self) {
        // nop
    }

    /// 是否应该在插件被禁用后`drop`插件实例
    /// 若为`false`，则插件只会在卸载时`drop`
    fn should_drop() -> bool {
        true
    }
}

pub trait IntoInstance {
    fn into_instance(self) -> PluginInstance;
}

impl<P: Plugin> IntoInstance for P {
    fn into_instance(self) -> PluginInstance {
        extern fn _enable<P: Plugin>(ptr: *mut ()) {
            // Safety: Plugin is pinned by box
            let p = unsafe { &mut *(ptr as *mut P) };
            p.enable();
        }

        extern fn _disable<P: Plugin>(ptr: *mut ()) {
            // Safety: Plugin is pinned by box
            let p = unsafe { &mut *(ptr as *mut P) };
            p.disable();
        }

        let instance = Managed::from_value(self);
        let vtb = PluginVTable::from(_enable::<Self>, _disable::<Self>);

        PluginInstance::from(instance, vtb)
    }
}