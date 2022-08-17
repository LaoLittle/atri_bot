use crate::event::{FriendMessageEvent, GroupMessageEvent};
use crate::plugin::cast_ref;
use atri_ffi::contact::FFIMember;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::Managed;
use std::sync::atomic::{AtomicBool, Ordering};

pub extern "C" fn event_intercept(intercepted: *const ()) {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.swap(true, Ordering::Release);
}

pub extern "C" fn event_is_intercepted(intercepted: *const ()) -> bool {
    let intercepted = unsafe { &*(intercepted as *const AtomicBool) };
    intercepted.load(Ordering::Relaxed)
}

pub extern "C" fn group_message_event_get_group(event: *const ()) -> Managed {
    let event: &GroupMessageEvent = cast_ref(event);
    Managed::from_value(event.group().clone())
}

pub extern "C" fn group_message_event_get_message(event: *const ()) -> FFIMessageChain {
    let event: &GroupMessageEvent = cast_ref(event);
    let chain = event.message().elements.clone();
    let chain = crate::message::MessageChain::from(chain);
    chain.into_ffi()
}

pub extern "C" fn group_message_event_get_sender(event: *const ()) -> FFIFuture<FFIMember> {
    FFIFuture::from(async {
        let event: &GroupMessageEvent = cast_ref(event);
        let sender = event.sender().await;
        sender.into_ffi()
    })
}

pub extern "C" fn friend_message_event_get_friend(event: *const ()) -> Managed {
    let event: &FriendMessageEvent = cast_ref(event);
    Managed::from_value(event.friend().clone())
}

pub extern "C" fn friend_message_event_get_message(event: *const ()) -> FFIMessageChain {
    let event: &FriendMessageEvent = cast_ref(event);
    let chain = event.message().elements.clone();
    let chain = crate::message::MessageChain::from(chain);
    chain.into_ffi()
}
