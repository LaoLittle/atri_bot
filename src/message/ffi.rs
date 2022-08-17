use super::MessageChain;
use crate::message::MessageValue;
use atri_ffi::message::{FFIMessageChain, FFIMessageValue, MessageValueUnion};
use atri_ffi::{Managed, RawVec, RustString};
use std::mem::ManuallyDrop;

impl MessageChain {
    pub fn into_ffi(self) -> FFIMessageChain {
        let ffi: Vec<FFIMessageValue> = self.0.into_iter().map(FFIMessageValue::from).collect();

        let raw = RawVec::from(ffi);
        FFIMessageChain { inner: raw }
    }

    pub fn from_ffi(ffi: FFIMessageChain) -> Self {
        let v = ffi.inner.into_vec();

        let values: Vec<MessageValue> = v.into_iter().map(MessageValue::from).collect();
        Self(values)
    }
}

impl From<MessageValue> for FFIMessageValue {
    fn from(val: MessageValue) -> Self {
        match val {
            MessageValue::Text(s) => FFIMessageValue {
                t: 0,
                union: MessageValueUnion {
                    text: ManuallyDrop::new(RustString::from(s)),
                },
            },
            MessageValue::Image(img) => FFIMessageValue {
                t: 1,
                union: MessageValueUnion {
                    image: ManuallyDrop::new(Managed::from_value(img)),
                },
            },
            or => FFIMessageValue {
                t: 255,
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(Managed::from_value(or)),
                },
            },
        }
    }
}

impl From<FFIMessageValue> for MessageValue {
    fn from(v: FFIMessageValue) -> Self {
        unsafe {
            match v.t {
                0 => MessageValue::Text(ManuallyDrop::into_inner(v.union.text).into()),
                1 => MessageValue::Image(ManuallyDrop::into_inner(v.union.image).into_value()),
                _ => ManuallyDrop::into_inner(v.union.unknown).into_value(),
            }
        }
    }
}
