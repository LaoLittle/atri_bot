use crate::{Managed, RawVec, RustString};
use std::mem::ManuallyDrop;

#[repr(C)]
pub struct FFIMessageChain {
    pub inner: RawVec<FFIMessageValue>,
}

#[repr(C)]
pub struct FFIMessageValue {
    pub t: u8,
    pub union: MessageValueUnion,
}

#[repr(C)]
pub union MessageValueUnion {
    pub text: ManuallyDrop<RustString>,
    pub image: ManuallyDrop<Managed>,
    pub reply: ManuallyDrop<FFIReply>,
    pub at: ManuallyDrop<FFIAt>,
    pub unknown: ManuallyDrop<Managed>,
}

#[repr(C)]
pub struct FFIImage {
    pub t: u8,
    pub union: ImageUnion,
}

#[repr(C)]
pub union ImageUnion {
    pub group: ManuallyDrop<Managed>,
    pub friend: ManuallyDrop<Managed>,
}

#[repr(C)]
pub struct FFIAt {
    pub target: i64,
    pub display: RustString,
}

#[repr(C)]
pub struct FFIReply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: FFIMessageChain,
}