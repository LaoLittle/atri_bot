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

impl From<RustString> for FFIMessageValue {
    fn from(t: RustString) -> Self {
        Self {
            t: 0,
            union: MessageValueUnion {
                text: ManuallyDrop::new(t),
            },
        }
    }
}

#[repr(C)]
pub union MessageValueUnion {
    pub text: ManuallyDrop<RustString>,
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
