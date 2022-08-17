mod bot;
mod event;
mod friend;
mod group;
mod listener;
mod log;
mod member;
mod message;

use std::sync::OnceLock;

use crate::plugin::ffi::bot::bot_get_id;
use crate::plugin::ffi::event::{
    event_intercept, event_is_intercepted, friend_message_event_get_friend,
    friend_message_event_get_message, group_message_event_get_group,
    group_message_event_get_message, group_message_event_get_sender,
};
use crate::plugin::ffi::group::{
    group_get_bot, group_get_id, group_get_name, group_quit, group_send_message, group_upload_image,
};
use crate::plugin::ffi::listener::new_listener;

use crate::plugin::ffi::friend::{
    friend_get_bot, friend_get_id, friend_get_nickname, friend_send_message,
};
use crate::plugin::ffi::log::log_info;
use crate::PluginManager;
use atri_ffi::ffi::AtriVTable;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use crate::plugin::ffi::member::{named_member_change_card_name, named_member_get_card_name, named_member_get_group, named_member_get_id, named_member_get_nickname};

static PLUGIN_VTABLE: OnceLock<AtriVTable> = OnceLock::new();

pub fn get_plugin_vtable() -> *const AtriVTable {
    PLUGIN_VTABLE.get_or_init(|| AtriVTable {
        plugin_manager_spawn,
        plugin_manager_block_on,
        new_listener,
        event_intercept,
        event_is_intercepted,
        bot_get_id,
        group_message_event_get_group,
        group_message_event_get_message,
        group_message_event_get_sender,
        group_get_id,
        group_get_name,
        group_get_bot,
        group_send_message,
        group_upload_image,
        group_quit,
        friend_message_event_get_friend,
        friend_message_event_get_message,
        friend_get_id,
        friend_get_nickname,
        friend_get_bot,
        friend_send_message,
        named_member_get_id,
        named_member_get_nickname,
        named_member_get_card_name,
        named_member_get_group,
        named_member_change_card_name,
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
