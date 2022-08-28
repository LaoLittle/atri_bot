mod bot;
mod event;
mod friend;
mod group;
mod listener;
mod log;
mod member;
mod message;

use atri_ffi::error::FFIResult;
use std::ptr::null;

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
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

pub extern "C" fn plugin_get_function(sig: u16) -> *const () {
    extern "C" fn not_impl() {
        panic!("No such sig");
    }

    macro_rules! match_function {
        (input: $input:expr; $($sig:expr => $fun:expr),* $(,)?) => {
            match $input {
                $($sig => $fun as *const (),)*
                _ => not_impl as *const (),
            }
        };
    }

    match_function! {
        input: sig;
        // plugin manager
        0 => plugin_manager_spawn,
        1 => plugin_manager_block_on,

        // listener
        100 => new_listener,

        // event
        200 => event_intercept,
        201 => event_is_intercepted,

        // bot
        300 => bot_get_id,
        301 => bot_get_nickname,
        302 => bot_get_list,
        303 => find_bot,
        304 => bot_find_group,
        305 => bot_find_friend,
        306 => bot_get_groups,
        307 => bot_get_friends,

        // group
        400 => group_get_id,
        401 => group_get_name,
        402 => group_get_bot,
        403 => group_get_members,
        404 => group_find_member,
        405 => group_get_named_member,
        406 => group_send_message,
        407 => group_upload_image,
        408 => group_quit,
        409 => group_change_name,

        // friend
        500 => friend_get_id,
        501 => friend_get_nickname,
        502 => friend_get_bot,
        503 => friend_send_message,
        504 => friend_upload_image,

        // named member
        600 => named_member_get_id,
        601 => named_member_get_nickname,
        602 => named_member_get_card_name,
        603 => named_member_get_group,
        604 => named_member_change_card_name,

        // group message event
        10000 => group_message_event_get_group,
        10001 => group_message_event_get_message,
        10002 => group_message_event_get_sender,

        // friend message event
        10100 => friend_message_event_get_friend,
        10101 => friend_message_event_get_message,

        // log
        20000 => log_info,
    }
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
