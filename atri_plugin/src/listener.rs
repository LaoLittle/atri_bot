use std::future::Future;
use atri_ffi::closure::FFIFn;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use crate::event::Event;
use crate::loader::get_plugin_manager_vtb;

pub struct Listener;

impl Listener {
    pub fn new<F, Fu>(handler: F) -> ListenerGuard
        where
            F: Fn(Event) -> Fu,
            F: Send + 'static,
            Fu: Future<Output = bool>,
            Fu: Send + 'static,
    {
        let f = FFIFn::from(|ffi| FFIFuture::from(handler(Event::from_ffi(ffi))));
        let ma = (get_plugin_manager_vtb().new_listener)(f);
        ListenerGuard(ma)
    }

    fn new_always<F, Fu>(handler: F) -> ListenerGuard
        where
            F: Fn(Event) -> Fu,
            F: Send + 'static,
            Fu: Future<Output = ()>,
            Fu: Send + 'static,
    {
        Self::new(move |e: Event| {
            let fu = handler(e);
            let b: Box<dyn Future<Output = bool> + Send + 'static> = Box::new(async move {
                fu.await;
                true
            });

            Box::into_pin(b)
        })
    }
}

pub struct ListenerGuard(Managed);