mod bot;
mod event;
mod group;
mod listener;
mod log;
mod message;

use std::sync::OnceLock;

use crate::plugin::ffi::bot::bot_get_id;
use crate::plugin::ffi::event::{
    event_intercept, event_is_intercepted, group_message_event_get_bot,
    group_message_event_get_group, group_message_event_get_message,
};
use crate::plugin::ffi::group::{group_get_id, group_send_message, group_upload_image};
use crate::plugin::ffi::listener::new_listener;

use crate::plugin::ffi::log::log_info;
use crate::PluginManager;
use atri_ffi::ffi::AtriVTable;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

static PLUGIN_VTABLE: OnceLock<AtriVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const AtriVTable {
    PLUGIN_VTABLE.get_or_init(|| AtriVTable {
        plugin_manager_spawn,
        plugin_manager_block_on,
        new_listener,
        event_intercept,
        event_is_intercepted,
        bot_get_id,
        group_message_event_get_bot,
        group_message_event_get_group,
        group_message_event_get_message,
        group_get_id,
        group_send_message,
        group_upload_image,
        log_info,
    })
}

extern "C" fn plugin_manager_spawn(
    manager: *const (),
    future: FFIFuture<Managed>,
) -> FFIFuture<Managed> {
    let manager = unsafe { &*(manager as *const PluginManager) };
    let handle = manager.async_runtime().spawn(future);

    FFIFuture::from(async { handle.await.unwrap() })
}

extern "C" fn plugin_manager_block_on(manager: *const (), future: FFIFuture<Managed>) -> Managed {
    let manager = unsafe { &*(manager as *const PluginManager) };
    manager.async_runtime().block_on(future)
}
