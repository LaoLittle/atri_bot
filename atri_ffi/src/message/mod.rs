use crate::message::meta::{FFIMessageMetadata, FFIReply};
use crate::{Managed, RawVec, RustString};
use std::mem::ManuallyDrop;

pub mod meta;

pub const TEXT: u8 = 0;
pub const IMAGE: u8 = 1;
pub const AT: u8 = 3;
pub const UNKNOWN: u8 = 255;

#[repr(C)]
pub struct FFIMessageChain {
    pub meta: FFIMessageMetadata,
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
