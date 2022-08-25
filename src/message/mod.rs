pub mod at;
pub mod ffi;
pub mod image;
pub mod meta;

use crate::message::at::At;
use crate::message::meta::{Anonymous, MessageMetadata, MetaMessage, Reply};
use crate::Text;
use core::slice;
use image::Image;
use ricq::msg::elem::RQElem;
use ricq::msg::{MessageElem, PushElem};
use ricq::structs::{FriendMessage, GroupMessage};
use std::vec;

#[derive(Clone, Default)]
pub struct MessageChain {
    meta: MessageMetadata,
    value: Vec<MessageValue>,
}

impl MessageChain {
    pub fn iter(&self) -> slice::Iter<'_, MessageValue> {
        self.into_iter()
    }

    pub fn metadata(&self) -> &MessageMetadata {
        &self.meta
    }
}

impl MetaMessage for MessageChain {
    fn metadata(&self) -> &MessageMetadata {
        &self.meta
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
    type IntoIter = slice::Iter<'a, MessageValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

impl From<GroupMessage> for MessageChain {
    fn from(g: GroupMessage) -> Self {
        let mut ran = Self::from(g.elements);
        ran.meta = MessageMetadata {
            seqs: g.seqs,
            rands: g.rands,
            time: g.time,
            sender: g.from_uin,
            ..ran.meta
        };

        ran
    }
}

impl From<FriendMessage> for MessageChain {
    fn from(f: FriendMessage) -> Self {
        let mut ran = Self::from(f.elements);
        ran.meta = MessageMetadata {
            seqs: f.seqs,
            rands: f.rands,
            time: f.time,
            sender: f.from_uin,
            ..ran.meta
        };

        ran
    }
}

impl From<Vec<MessageValue>> for MessageChain {
    fn from(elems: Vec<MessageValue>) -> Self {
        Self {
            value: elems,
            ..Default::default()
        }
    }
}

impl From<ricq::msg::MessageChain> for MessageChain {
    fn from(chain: ricq::msg::MessageChain) -> Self {
        let mut iter = chain.0.into_iter();

        let mut meta = MessageMetadata::default();
        let mut value: Vec<MessageValue> = vec![];

        for _ in 0..2 {
            match iter.next() {
                Some(MessageElem::AnonGroupMsg(msg)) => {
                    let rq = ricq::msg::elem::Anonymous::from(msg);
                    meta.anonymous = Some(Anonymous::from(rq));
                }
                Some(MessageElem::SrcMsg(src)) => {
                    let rq = ricq::msg::elem::Reply::from(src);
                    meta.reply = Some(Reply::from(rq));
                }
                Some(or) => {
                    let rq = ricq::msg::elem::RQElem::from(or);
                    value.push(MessageValue::from(rq));
                }
                None => {}
            }
        }

        for val in iter {
            let rq = ricq::msg::elem::RQElem::from(val);
            value.push(MessageValue::from(rq));
        }

        Self { meta, value }
    }
}

impl From<MessageChain> for ricq::msg::MessageChain {
    fn from(chain: MessageChain) -> Self {
        let mut rq = ricq::msg::MessageChain::default();
        MessageChain::push_to(chain, &mut rq.0);
        rq
    }
}

impl PushElem for MessageChain {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        if let Some(reply) = elem.meta.reply {
            let rq = ricq::msg::elem::Reply::from(reply);
            vec.push(rq.into());
        }

        if let Some(ano) = elem.meta.anonymous {
            let rq = ricq::msg::elem::Anonymous::from(ano);
            vec.push(rq.into());
        }

        for value in elem.value {
            MessageValue::push_to(value, vec);
        }
    }
}

#[derive(Clone)]
pub enum MessageValue {
    Text(String),
    Image(Image),
    At(At),
    Unknown(RQElem),
}

impl From<MessageValue> for RQElem {
    fn from(val: MessageValue) -> Self {
        match val {
            MessageValue::Text(s) => RQElem::Text(Text { content: s }),
            MessageValue::Image(img) => match img {
                Image::Friend(img) => RQElem::FriendImage(img),
                Image::Group(img) => RQElem::GroupImage(img),
            },
            MessageValue::At(At { target, display }) => {
                RQElem::At(ricq::msg::elem::At { target, display })
            }
            MessageValue::Unknown(rq) => rq,
        }
    }
}

impl From<RQElem> for MessageValue {
    fn from(elem: RQElem) -> Self {
        match elem {
            RQElem::Text(Text { content }) => MessageValue::Text(content),
            RQElem::GroupImage(img) => MessageValue::Image(Image::Group(img)),
            RQElem::FriendImage(img) => MessageValue::Image(Image::Friend(img)),
            RQElem::At(at) => MessageValue::At(At {
                target: at.target,
                display: at.display,
            }),
            or => Self::Unknown(or),
        }
    }
}

impl PushElem for MessageValue {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        match elem {
            Self::Text(s) => PushElem::push_to(Text::new(s), vec),
            Self::Image(img) => PushElem::push_to(img, vec),
            Self::At(at) => PushElem::push_to(at, vec),
            _ => {}
        }
    }
}
