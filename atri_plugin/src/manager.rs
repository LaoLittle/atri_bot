use std::future::Future;
use atri_ffi::ffi::JoinHandle;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use crate::loader::get_plugin_vtable;

pub struct PluginManager;

impl PluginManager {
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future,
        F: Send + 'static,
    {
        let ffi = FFIFuture::from(async move {
            let value: F::Output = future.await;

            Managed::from_value(value)
        });

        let f = (get_plugin_vtable().plugin_manager_spawn)(ffi);
        let handle = JoinHandle::<F::Output>::from(f);
        handle
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let ffi = FFIFuture::from(async move {
            let value: F::Output = future.await;

            Managed::from_value(value)
        });
        let managed = (get_plugin_vtable().plugin_manager_block_on)(ffi);
        managed.into_value()
    }
}