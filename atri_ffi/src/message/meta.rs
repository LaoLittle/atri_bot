use crate::message::FFIMessageValue;
use crate::{RawVec, RustString};
use std::mem::MaybeUninit;

pub const NONE_META: u8 = 0;
pub const ANONYMOUS_FLAG: u8 = 1;
pub const REPLY_FLAG: u8 = 2;
pub const ALL_META: u8 = ANONYMOUS_FLAG | REPLY_FLAG;

#[repr(C)]
pub struct FFIMessageMetadata {
    pub flags: u8,
    pub anonymous: MaybeUninit<FFIAnonymous>,
    pub reply: MaybeUninit<FFIReply>,
}

#[repr(C)]
pub struct FFIAnonymous {
    pub anon_id: RawVec<u8>,
    pub nick: RustString,
    pub portrait_index: i32,
    pub bubble_index: i32,
    pub expire_time: i32,
    pub color: RustString,
}

#[repr(C)]
pub struct FFIReply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: RawVec<FFIMessageValue>,
}
