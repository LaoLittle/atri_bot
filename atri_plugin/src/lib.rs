use atri_ffi::plugin::{PluginVTable};
use atri_ffi::Managed;

pub use atri_macros::plugin;

pub mod loader;
pub mod manager;
pub mod listener;
pub mod event;
pub mod bot;
pub mod contact;

pub use atri_ffi::plugin::PluginInstance;

pub trait Plugin
where Self: Sized
{
    /// 构造插件实例
    ///
    /// 若`should_drop`为`true`, 则再次启用插件前会先构造插件实例
    fn new() -> Self;

    /// 插件启用
    fn enable(&mut self);

    /// 插件禁用
    fn disable(&mut self) {
        // 默认实现: nop
    }

    /// 是否应该在插件被禁用后销毁插件实例
    ///
    /// 若为`false`，则插件只会在卸载时销毁实例
    fn should_drop() -> bool {
        false
    }
}

pub fn __get_instance<P: Plugin>(plugin: P) -> PluginInstance {
    extern fn _new<P: Plugin>() -> Managed {
        Managed::from_value(P::new())
    }

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

    let should_drop = P::should_drop();

    let instance = Managed::from_value(plugin);
    let vtb = PluginVTable {
        new: _new::<P>,
        enable: _enable::<P>,
        disable: _disable::<P>
    };

    PluginInstance::from(instance, should_drop,vtb)
}