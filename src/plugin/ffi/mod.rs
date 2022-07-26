use std::sync::OnceLock;

use crate::plugin::ffi::channel::new_receiver;
use crate::plugin::ffi::channel::receiver_receive;
use crate::plugin::ffi::event::FFIEvent;
use crate::plugin::future::FFIFuture;
use crate::plugin::Managed;

mod bot;
mod channel;
pub mod event;
mod listener;

#[repr(C)]
pub struct PluginVTable {
    new_receiver: extern fn() -> Managed,
    receiver_receive: extern fn(*mut ()) -> FFIFuture<FFIEvent>,
}

static PLUGIN_VTABLE: OnceLock<PluginVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const PluginVTable {
    PLUGIN_VTABLE.get_or_init(|| {
        PluginVTable {
            new_receiver,
            receiver_receive,
        }
    }) as _
}