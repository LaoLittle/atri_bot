use std::mem::ManuallyDrop;
use std::slice::Iter;
use std::{vec};
use atri_ffi::{Managed, RawVec, RustString};

use atri_ffi::message::{FFIMessageChain, FFIMessageValue, MessageValueUnion};


pub struct MessageChain(Vec<MessageValue>);

impl MessageChain {
    pub fn iter(&self) -> Iter<'_, MessageValue> {
        self.into_iter()
    }
    
    pub(crate) fn from_ffi(ffi: FFIMessageChain) -> Self {
        let v = ffi.inner.into_vec();
        Self(v.into_iter().map(MessageValue::from).collect())
    }
    
    pub(crate) fn into_ffi(self) -> FFIMessageChain {
        let v: Vec<FFIMessageValue> = self.0.into_iter().map(FFIMessageValue::from).collect();
        let raw = RawVec::from(v);
        FFIMessageChain { inner: raw }
    }
}

impl IntoIterator for MessageChain {
    type Item = MessageValue;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MessageChain {
    type Item = &'a MessageValue;
    type IntoIter = Iter<'a, MessageValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

pub enum MessageValue {
    Text(String),
    Unknown(Managed)
}

impl From<FFIMessageValue> for MessageValue {
    fn from(ffi: FFIMessageValue) -> Self {
        unsafe {
            match ffi.t {
                0 => Self::Text(ManuallyDrop::into_inner(ffi.union.text).into()),
                _ => Self::Unknown(ManuallyDrop::into_inner(ffi.union.unknown))
            }
        }
    }
}

impl From<MessageValue> for FFIMessageValue {
    fn from(val: MessageValue) -> Self {
        match val {
            MessageValue::Text(s) => {
                Self {
                    t: 0,
                    union: MessageValueUnion { text: ManuallyDrop::new(RustString::from(s)) }
                }
            }
            MessageValue::Unknown(ma) => Self {
                t: 255,
                union: MessageValueUnion { unknown: ManuallyDrop::new(ma) }
            }
        }
    }
}

#[derive(Default)]
pub struct MessageChainBuilder {
    pub value: Vec<MessageValue>,
    pub buf: String,
}

impl MessageChainBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self) -> &mut Self {
        self
    }

    pub fn push_str(&mut self, str: &str) -> &mut Self {
        self.buf.push_str(str);
        self
    }

    pub fn build(mut self) -> MessageChain {
        let text = MessageValue::Text(self.buf);
        self.value.push(text);
        MessageChain(self.value)
    }
}