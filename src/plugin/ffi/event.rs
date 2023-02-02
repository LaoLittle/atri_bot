use super::cast_ref;
use crate::event::{FriendMessageEvent, GroupMessageEvent};
use atri_ffi::contact::FFIMember;
use atri_ffi::ffi::ForFFI;

use crate::contact::friend::Friend;
use crate::contact::group::Group;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::PHandle;
use std::sync::atomic::{AtomicBool, Ordering};

pub extern "C" fn event_intercept(intercepted: *const ()) {
    let intercepted: &AtomicBool = cast_ref(intercepted);
    intercepted.store(true, Ordering::Relaxed);
}

pub extern "C" fn event_is_intercepted(intercepted: *const ()) -> bool {
    let intercepted: &AtomicBool = cast_ref(intercepted);
    intercepted.load(Ordering::Relaxed)
}

pub extern "C" fn group_message_event_get_group(event: *const ()) -> PHandle {
    let event: &GroupMessageEvent = cast_ref(event);
    event.group() as *const Group as PHandle
}

pub extern "C" fn group_message_event_get_message(event: *const ()) -> FFIMessageChain {
    let event: &GroupMessageEvent = cast_ref(event);
    let chain = event.message().to_owned();
    chain.into_ffi()
}

pub extern "C" fn group_message_event_get_sender(event: *const ()) -> FFIMember {
    let event: &GroupMessageEvent = cast_ref(event);
    let sender = event.sender().to_owned();
    sender.into_ffi()
}

pub extern "C" fn friend_message_event_get_friend(event: *const ()) -> PHandle {
    let event: &FriendMessageEvent = cast_ref(event);
    event.friend() as *const Friend as PHandle
}

pub extern "C" fn friend_message_event_get_message(event: *const ()) -> FFIMessageChain {
    let event: &FriendMessageEvent = cast_ref(event);
    let chain = event.message().to_owned();
    chain.into_ffi()
}
