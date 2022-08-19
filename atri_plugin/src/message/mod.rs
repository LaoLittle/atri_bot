pub mod meta;
mod ffi;
pub mod at;

use atri_ffi::{Managed};

use std::slice::Iter;
use std::{mem, vec};
use crate::message::at::At;
use crate::message::meta::{MessageMetadata};


#[derive(Default)]
pub struct MessageChain {
    meta: MessageMetadata,
    value: Vec<MessageValue>,
}

impl MessageChain {
    pub fn iter(&self) -> Iter<'_, MessageValue> {
        self.into_iter()
    }
}

impl IntoIterator for MessageChain {
    type Item = MessageValue;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<'a> IntoIterator for &'a MessageChain {
    type Item = &'a MessageValue;
    type IntoIter = Iter<'a, MessageValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
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
    At(At),
    Unknown(Managed),
}

impl MessageValue {
    fn push_to_string(&self, str: &mut String) {
        match self {
            Self::Text(text) => str.push_str(text),
            Self::Image(_) => str.push_str("Image"),
            Self::At(At {
                         display,..
                     }) => str.push_str(display),
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
        self.flush();
        elem.push_to(&mut self.value);
        self
    }

    pub fn push_str(&mut self, str: &str) -> &mut Self {
        self.buf.push_str(str);
        self
    }

    pub fn build(mut self) -> MessageChain {
        self.flush();
        MessageChain {
            value: self.value,
            ..Default::default()
        }
    }

    fn flush(&mut self) {
        let buf = mem::take(&mut self.buf);
        let text = MessageValue::Text(buf);
        self.value.push(text);
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

impl Message for At {
    fn push_to(self, v: &mut Vec<MessageValue>) {
        v.push(MessageValue::At(self));
    }
}