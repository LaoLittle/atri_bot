mod client;
mod env;
mod event;
mod friend;
mod group;
mod listener;
mod log;
mod member;
mod message;

use crate::plugin::ffi::client::{
    client_find_friend, client_find_group, client_get_friends, client_get_groups, client_get_id,
    client_get_list, client_get_nickname, find_client,
};
use crate::plugin::ffi::event::{
    event_intercept, event_is_intercepted, friend_message_event_get_friend,
    friend_message_event_get_message, group_message_event_get_group,
    group_message_event_get_message, group_message_event_get_sender,
};
use crate::plugin::ffi::group::{
    group_change_name, group_change_name_blocking, group_find_member, group_find_or_refresh_member,
    group_get_client, group_get_id, group_get_members, group_get_name, group_quit,
    group_quit_blocking, group_send_message, group_send_message_blocking, group_upload_image,
    group_upload_image_blocking,
};
use crate::plugin::ffi::listener::{
    listener_next_event_with_priority, listener_next_event_with_priority_blocking, new_listener,
    new_listener_c_func, new_listener_closure,
};
use atri_ffi::error::FFIResult;
use std::future::Future;

use crate::plugin::cast_ref;
use crate::plugin::ffi::env::env_get_workspace;
use crate::plugin::ffi::friend::{
    friend_get_bot, friend_get_id, friend_get_nickname, friend_send_message,
    friend_send_message_blocking, friend_upload_image, friend_upload_image_blocking,
};
use crate::plugin::ffi::log::log;
use crate::plugin::ffi::member::{
    named_member_change_card_name, named_member_change_card_name_blocking,
    named_member_get_card_name, named_member_get_group, named_member_get_id,
    named_member_get_nickname,
};
use crate::plugin::ffi::message::{
    image_get_id, image_get_url, message_chain_from_json, message_chain_to_json,
};
use crate::PluginManager;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;

pub extern "C" fn plugin_get_function(sig: u16) -> *const () {
    extern "C" fn not_impl() {
        panic!("No such operation");
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
        101 => listener_next_event_with_priority,
        150 => new_listener_c_func,
        151 => new_listener_closure,
        152 => listener_next_event_with_priority_blocking,

        // event
        200 => event_intercept,
        201 => event_is_intercepted,

        // client
        300 => client_get_id,
        301 => client_get_nickname,
        302 => client_get_list,
        303 => find_client,
        304 => client_find_group,
        305 => client_find_friend,
        306 => client_get_groups,
        307 => client_get_friends,

        // group
        400 => group_get_id,
        401 => group_get_name,
        402 => group_get_client,
        403 => group_get_members,
        404 => group_find_member,
        405 => group_find_or_refresh_member,
        406 => group_send_message,
        407 => group_upload_image,
        408 => group_quit,
        409 => group_change_name,

        // blocking api
        456 => group_send_message_blocking,
        457 => group_upload_image_blocking,
        458 => group_quit_blocking,
        459 => group_change_name_blocking,

        // friend
        500 => friend_get_id,
        501 => friend_get_nickname,
        502 => friend_get_bot,
        503 => friend_send_message,
        504 => friend_upload_image,

        // blocking api
        553 => friend_send_message_blocking,
        554 => friend_upload_image_blocking,

        // named member
        600 => named_member_get_id,
        601 => named_member_get_nickname,
        602 => named_member_get_card_name,
        603 => named_member_get_group,
        604 => named_member_change_card_name,

        // blocking api
        654 => named_member_change_card_name_blocking,


        // group message event
        10000 => group_message_event_get_group,
        10001 => group_message_event_get_message,
        10002 => group_message_event_get_sender,

        // friend message event
        10100 => friend_message_event_get_friend,
        10101 => friend_message_event_get_message,

        2000 => image_get_id,
        // flash => 2001
        2002 => image_get_url,

        // log
        20000 => log,

        // env
        30000 => env_get_workspace,

        // serialize
        30100 => message_chain_to_json,
        30101 => message_chain_from_json,
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

fn future_block_on<F>(manager: *const (), future: F) -> F::Output
where
    F: Future,
    F: Send + 'static,
    F::Output: Send + 'static,
{
    let manager: &PluginManager = cast_ref(manager);

    let (tx, rx) = std::sync::mpsc::channel();

    manager.async_runtime().spawn(async move {
        let val = future.await;
        let _ = tx.send(val);
    });

    let rx = || rx.recv().expect("Cannot recv");
    // calling this outside a runtime normally calls the provided closure.
    // all runtime is multi-threaded
    tokio::task::block_in_place(rx)
}
