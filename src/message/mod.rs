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
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::vec;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MessageChain {
    meta: MessageMetadata,
    value: Vec<MessageElement>,
}

impl MessageChain {
    pub fn iter(&self) -> slice::Iter<'_, MessageElement> {
        self.into_iter()
    }

    pub fn metadata(&self) -> &MessageMetadata {
        &self.meta
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Serializing error")
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

impl MetaMessage for MessageChain {
    fn metadata(&self) -> &MessageMetadata {
        &self.meta
    }
}

impl ToString for MessageChain {
    fn to_string(&self) -> String {
        self.iter().map(|value| value.to_string()).collect()
    }
}

impl Debug for MessageChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string())
    }
}

impl IntoIterator for MessageChain {
    type Item = MessageElement;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<'a> IntoIterator for &'a MessageChain {
    type Item = &'a MessageElement;
    type IntoIter = slice::Iter<'a, MessageElement>;

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

impl From<Vec<MessageElement>> for MessageChain {
    fn from(elems: Vec<MessageElement>) -> Self {
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
        let mut value: Vec<MessageElement> = vec![];

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
                    value.push(MessageElement::from(rq));
                }
                None => {}
            }
        }

        for val in iter {
            let rq = ricq::msg::elem::RQElem::from(val);
            value.push(MessageElement::from(rq));
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
            MessageElement::push_to(value, vec);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum MessageElement {
    Text(String),
    Image(Image),
    At(At),
    AtAll,
    #[serde(skip)]
    Unknown(RQElem),
}

impl ToString for MessageElement {
    fn to_string(&self) -> String {
        let mut s = String::new();
        match self {
            Self::Text(t) => s.push_str(t),
            Self::Image(img) => s.push_str(&format!("$[Image:{}]", img.url())),
            Self::At(At { target, display }) => {
                s.push_str(&format!("$[At:{}({})]", display, target))
            }
            Self::AtAll => s.push_str("$[AtAll]"),
            Self::Unknown(rq) => s.push_str(&rq.to_string()),
        }
        s
    }
}

impl From<String> for MessageElement {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<MessageElement> for RQElem {
    fn from(val: MessageElement) -> Self {
        match val {
            MessageElement::Text(s) => RQElem::Text(Text { content: s }),
            MessageElement::Image(img) => match img {
                Image::Friend(img) => RQElem::FriendImage(img),
                Image::Group(img) => RQElem::GroupImage(img),
            },
            MessageElement::At(at) => RQElem::At(at.into()),
            MessageElement::AtAll => RQElem::At(At::ALL.into()),
            MessageElement::Unknown(rq) => rq,
        }
    }
}

impl From<RQElem> for MessageElement {
    fn from(elem: RQElem) -> Self {
        match elem {
            RQElem::Text(Text { content }) => MessageElement::Text(content),
            RQElem::GroupImage(img) => MessageElement::Image(Image::Group(img)),
            RQElem::FriendImage(img) => MessageElement::Image(Image::Friend(img)),
            RQElem::At(at) => {
                if at.target == 0 {
                    MessageElement::AtAll
                } else {
                    MessageElement::At(At {
                        target: at.target,
                        display: at.display,
                    })
                }
            }
            or => Self::Unknown(or),
        }
    }
}

impl PushElem for MessageElement {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        match elem {
            Self::Text(s) => PushElem::push_to(Text::new(s), vec),
            Self::Image(img) => PushElem::push_to(img, vec),
            Self::At(at) => PushElem::push_to(at, vec),
            Self::AtAll => PushElem::push_to(At::ALL, vec),
            Self::Unknown(_rq) => {}
        }
    }
}
