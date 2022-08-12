use std::future::Future;
use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use crate::loader::get_plugin_manager_vtb;

pub struct Listener;

impl Listener {
    pub fn new<F, Fu>(handler: F) -> ListenerGuard
        where
            F: Fn(FFIEvent) -> Fu,
            F: Send + 'static,
            Fu: Future<Output = bool>,
            Fu: Send + 'static,
    {
        let f = FFIFn::from(|m| FFIFuture::from(handler(m)));
        let ma = (get_plugin_manager_vtb().new_listener)(f);
        ListenerGuard(ma)
    }
}

pub struct ListenerGuard(Managed);