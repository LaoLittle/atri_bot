use crate::closure::FFIFn;
use crate::error::FFIResult;
use crate::future::FFIFuture;
use crate::message::FFIMessageChain;
use crate::{Managed, ManagedCloneable, RawVec, RustStr, RustString};

use crate::contact::FFIMember;

#[repr(C)]
pub struct AtriVTable {
    pub plugin_manager_spawn:
        extern "C" fn(manager: *const (), FFIFuture<Managed>) -> FFIFuture<FFIResult<Managed>>,
    pub plugin_manager_block_on: extern "C" fn(manager: *const (), FFIFuture<Managed>) -> Managed,

    pub new_listener: extern "C" fn(FFIFn<FFIEvent, FFIFuture<bool>>) -> Managed,

    pub event_intercept: extern "C" fn(intercepted: *const ()),
    pub event_is_intercepted: extern "C" fn(intercepted: *const ()) -> bool,

    pub bot_get_id: extern "C" fn(bot: *const ()) -> i64,
    pub bot_get_nickname: extern "C" fn(bot: *const ()) -> RustStr,

    pub group_message_event_get_group: extern "C" fn(event: *const ()) -> ManagedCloneable,
    pub group_message_event_get_message: extern "C" fn(event: *const ()) -> FFIMessageChain,
    pub group_message_event_get_sender: extern "C" fn(event: *const ()) -> FFIFuture<FFIMember>,

    pub group_get_id: extern "C" fn(group: *const ()) -> i64,
    pub group_get_name: extern "C" fn(group: *const ()) -> RustStr,
    pub group_get_bot: extern "C" fn(group: *const ()) -> ManagedCloneable,
    pub group_send_message:
        extern "C" fn(group: *const (), chain: FFIMessageChain) -> FFIFuture<FFIResult<Managed>>,
    pub group_upload_image:
        extern "C" fn(group: *const (), data: RawVec<u8>) -> FFIFuture<FFIResult<Managed>>,
    pub group_quit: extern "C" fn(group: *const ()) -> FFIFuture<bool>,

    pub friend_message_event_get_friend: extern "C" fn(event: *const ()) -> ManagedCloneable,
    pub friend_message_event_get_message: extern "C" fn(event: *const ()) -> FFIMessageChain,
    pub friend_get_id: extern "C" fn(friend: *const ()) -> i64,
    pub friend_get_nickname: extern "C" fn(friend: *const ()) -> RustStr,
    pub friend_get_bot: extern "C" fn(friend: *const ()) -> ManagedCloneable,
    pub friend_send_message:
        extern "C" fn(friend: *const (), chain: FFIMessageChain) -> FFIFuture<FFIResult<Managed>>,
    pub friend_upload_image:
        extern "C" fn(friend: *const (), img: RawVec<u8>) -> FFIFuture<FFIResult<Managed>>,

    pub named_member_get_id: extern "C" fn(named: *const ()) -> i64,
    pub named_member_get_nickname: extern "C" fn(named: *const ()) -> RustStr,
    pub named_member_get_card_name: extern "C" fn(named: *const ()) -> RustStr,
    pub named_member_get_group: extern "C" fn(named: *const ()) -> ManagedCloneable,
    pub named_member_change_card_name:
        extern "C" fn(named: *const (), card: RustString) -> FFIFuture<FFIResult<()>>,

    pub log_info: extern "C" fn(handle: usize, manager: *const (), log: RustString),
}

#[repr(C)]
pub struct AtriManager {
    pub manager_ptr: *const (),
    pub handle: usize,
    pub vtb: *const AtriVTable,
}

#[repr(C)]
pub struct FFIEvent {
    t: u8,
    intercepted: *const (),
    base: Managed,
}

impl FFIEvent {
    pub fn from(t: u8, intercepted: *const (), base: Managed) -> Self {
        Self {
            t,
            intercepted,
            base,
        }
    }

    pub fn get(self) -> (u8, *const (), Managed) {
        (self.t, self.intercepted, self.base)
    }
}

pub trait ForFFI {
    type FFIValue;

    fn into_ffi(self) -> Self::FFIValue;

    fn from_ffi(value: Self::FFIValue) -> Self;
}
