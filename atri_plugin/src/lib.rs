use std::future::Future;
use std::marker::PhantomData;
use atri_ffi::future::FFIFutureV;
use atri_ffi::Managed;
use atri_ffi::plugin::{PluginInstance, PluginVTable};

pub mod loader;

pub struct PluginBuilder<T> {
    user_data: Managed,
    name: Option<String>,
    enable: extern fn(*mut ()) -> FFIFutureV,
    disable: extern fn(*mut ()) -> FFIFutureV,
    _mark: PhantomData<T>,
}

impl<T> PluginBuilder<T> {
    pub fn new_with_data(val: T) -> Self {
        Self {
            user_data: Managed::from_value(val),
            name: None,
            enable: nop,
            disable: nop,
            _mark: PhantomData
        }
    }

    pub fn on_enable<F>(mut self, enable_fn: fn(&mut T) -> F)
    where F: Future<Output=()>
    {

    }

    pub fn on_disable<F>(mut self, disable_fn: fn(&mut T) -> F)
        where F: Future<Output=()>
    {


        extern fn _enable() {

        }
    }

    pub fn with_name(mut self,s: impl ToString) -> Self {
        self.name = Some(s.to_string());
        self
    }
}

impl PluginBuilder<()> {
    pub fn new_with_none() {

    }
}

extern fn nop(_: *mut ()) -> FFIFutureV {
    FFIFutureV::from(async {})
}

pub trait Plugin: Sized {
    fn enable(&mut self) -> FFIFutureV;

    fn disable(&mut self) -> FFIFutureV {
        // default impl
        FFIFutureV::from(async {})
    }

    fn into_instance(self) -> PluginInstance {
        let instance = Managed::from_value(self);
        let vtb = PluginVTable::from(
            _enable::<Self>,
            _disable::<Self>,
        );

        PluginInstance::from(instance, vtb)
    }
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