use atri_ffi::{Managed, RawVec, RustString};
use std::mem::ManuallyDrop;
use std::slice::Iter;
use std::{mem, vec};

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

impl ToString for MessageChain {
    fn to_string(&self) -> String {
        let mut s = String::new();
        for value in self {
            value.push_to_string(&mut s);
        }
        s
    }
}

pub enum MessageValue {
    Text(String),
    Image(Image),
    Unknown(Managed),
}

impl MessageValue {
    fn push_to_string(&self, str: &mut String) {
        match self {
            Self::Text(text) => str.push_str(text),
            Self::Image(_) => str.push_str("Image"),
            Self::Unknown(_) => {}
        }
    }
}

impl ToString for MessageValue {
    fn to_string(&self) -> String {
        let mut s = String::new();
        self.push_to_string(&mut s);
        s
    }
}

impl From<FFIMessageValue> for MessageValue {
    fn from(ffi: FFIMessageValue) -> Self {
        unsafe {
            match ffi.t {
                0 => Self::Text(ManuallyDrop::into_inner(ffi.union.text).into()),
                1 => Self::Image(Image(ManuallyDrop::into_inner(ffi.union.image))),
                _ => Self::Unknown(ManuallyDrop::into_inner(ffi.union.unknown)),
            }
        }
    }
}

impl From<MessageValue> for FFIMessageValue {
    fn from(val: MessageValue) -> Self {
        match val {
            MessageValue::Text(s) => Self {
                t: 0,
                union: MessageValueUnion {
                    text: ManuallyDrop::new(RustString::from(s)),
                },
            },
            MessageValue::Image(img) => Self {
                t: 1,
                union: MessageValueUnion {
                    image: ManuallyDrop::new(img.0),
                },
            },
            MessageValue::Unknown(ma) => Self {
                t: 255,
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(ma),
                },
            },
        }
    }
}

#[derive(Default)]
pub struct MessageChainBuilder {
    value: Vec<MessageValue>,
    buf: String,
}

impl MessageChainBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<E: Message>(&mut self, elem: E) -> &mut Self {
        let str = mem::take(&mut self.buf);
        let text = MessageValue::Text(str);
        self.value.push(text);
        elem.push_to(&mut self.value);
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

pub struct Image(pub(crate) Managed);

pub struct MessageReceipt(pub(crate) Managed);

pub trait Message {
    fn push_to(self, v: &mut Vec<MessageValue>);
}

impl Message for String {
    fn push_to(self, v: &mut Vec<MessageValue>) {
        v.push(MessageValue::Text(self));
    }
}

impl Message for Image {
    fn push_to(self, v: &mut Vec<MessageValue>) {
        v.push(MessageValue::Image(self));
    }
}
