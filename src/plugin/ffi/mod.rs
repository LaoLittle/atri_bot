mod bot;
mod event;
mod friend;
mod group;
mod listener;
mod log;
mod member;
mod message;

use atri_ffi::error::FFIResult;

use crate::plugin::ffi::bot::{
    bot_find_friend, bot_find_group, bot_get_friends, bot_get_groups, bot_get_id, bot_get_list,
    bot_get_nickname, find_bot,
};
use crate::plugin::ffi::event::{
    event_intercept, event_is_intercepted, friend_message_event_get_friend,
    friend_message_event_get_message, group_message_event_get_group,
    group_message_event_get_message, group_message_event_get_sender,
};
use crate::plugin::ffi::group::{
    group_change_name, group_find_member, group_get_bot, group_get_id, group_get_members,
    group_get_name, group_get_named_member, group_quit, group_send_message, group_upload_image,
};
use crate::plugin::ffi::listener::new_listener;

use crate::plugin::cast_ref;
use crate::plugin::ffi::friend::{
    friend_get_bot, friend_get_id, friend_get_nickname, friend_send_message, friend_upload_image,
};
use crate::plugin::ffi::log::log_info;
use crate::plugin::ffi::member::{
    named_member_change_card_name, named_member_get_card_name, named_member_get_group,
    named_member_get_id, named_member_get_nickname,
};
use crate::PluginManager;
use atri_ffi::ffi::AtriVTable;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

static PLUGIN_VTABLE: AtriVTable = AtriVTable {
    plugin_manager_spawn,
    plugin_manager_block_on,
    new_listener,
    event_intercept,
    event_is_intercepted,
    bot_get_id,
    bot_get_nickname,
    bot_get_list,
    find_bot,
    bot_find_group,
    bot_find_friend,
    bot_get_groups,
    bot_get_friends,
    group_message_event_get_group,
    group_message_event_get_message,
    group_message_event_get_sender,
    group_get_id,
    group_get_name,
    group_get_bot,
    group_get_members,
    group_find_member,
    group_get_named_member,
    group_send_message,
    group_upload_image,
    group_quit,
    group_change_name,
    friend_message_event_get_friend,
    friend_message_event_get_message,
    friend_get_id,
    friend_get_nickname,
    friend_get_bot,
    friend_send_message,
    friend_upload_image,
    named_member_get_id,
    named_member_get_nickname,
    named_member_get_card_name,
    named_member_get_group,
    named_member_change_card_name,
    log_info,
};

pub fn get_plugin_vtable() -> *const AtriVTable {
    &PLUGIN_VTABLE
}

extern "C" fn plugin_manager_spawn(
    manager: *const (),
    future: FFIFuture<Managed>,
) -> FFIFuture<FFIResult<Managed>> {
    let manager: &PluginManager = cast_ref(manager);
    let handle = manager.async_runtime().spawn(future);

    FFIFuture::from(async { FFIResult::from(handle.await) })
}

extern "C" fn plugin_manager_block_on(manager: *const (), future: FFIFuture<Managed>) -> Managed {
    let manager: &PluginManager = cast_ref(manager);
    manager.async_runtime().block_on(future)
}
