use std::mem::ManuallyDrop;
use std::slice::Iter;
use std::{vec};

use atri_ffi::message::{FFIMessageChain, FFIMessageValue};


pub struct MessageChain(Vec<MessageValue>);

impl MessageChain {
    pub fn iter(&self) -> Iter<'_, MessageValue> {
        self.into_iter()
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

impl From<FFIMessageChain> for MessageChain {
    fn from(ffi: FFIMessageChain) -> Self {
        let v = ffi.inner.into_vec();
        Self(v.into_iter().map(MessageValue::from).collect())
    }
}

pub enum MessageValue {
    Text(String),
    Unknown
}

impl From<FFIMessageValue> for MessageValue {
    fn from(ffi: FFIMessageValue) -> Self {
        unsafe {
            match ffi.t {
                0 => Self::Text(ManuallyDrop::into_inner(ffi.union.text).into()),
                _ => Self::Unknown
            }
        }
    }
}