pub mod at;
pub mod face;
pub mod ffi;
pub mod forward;
pub mod image;
pub mod macros;
pub mod meta;

use crate::message::at::At;
use crate::message::face::Face;
use crate::message::meta::{Anonymous, MessageMetadata, MessageReceipt, RecallMessage, Reply};
use crate::Text;
use core::slice;
use image::Image;
use ricq::msg::elem::RQElem;
use ricq::msg::{MessageElem, PushElem};
use ricq::structs::{FriendMessage, GroupMessage};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter, Write};
use std::vec;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MessageChain {
    meta: MessageMetadata,
    elements: Vec<MessageElement>,
}

impl MessageChain {
    pub fn iter(&self) -> slice::Iter<'_, MessageElement> {
        self.into_iter()
    }

    pub fn metadata(&self) -> &MessageMetadata {
        &self.meta
    }

    pub fn metadata_mut(&mut self) -> &mut MessageMetadata {
        &mut self.meta
    }

    pub fn referred(&self) -> Option<&Reply> {
        self.metadata().reply.as_ref()
    }

    pub fn reply(&self) -> Reply {
        Reply {
            reply_seq: self.meta.seqs[0],
            sender: self.meta.sender,
            time: self.meta.time,
            elements: self.elements.clone(),
        }
    }

    pub fn into_reply(self) -> Reply {
        Reply {
            reply_seq: self.meta.seqs[0],
            sender: self.meta.sender,
            time: self.meta.time,
            elements: self.elements,
        }
    }

    pub fn with_reply(&mut self, reply: Reply) {
        self.metadata_mut().reply = Some(reply)
    }

    pub fn with_anonymous(&mut self, ano: Anonymous) {
        self.metadata_mut().anonymous = Some(ano)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Serializing error")
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

impl RecallMessage for MessageChain {
    fn receipt(&self) -> MessageReceipt {
        MessageReceipt {
            seqs: self.metadata().seqs.clone(),
            rands: self.metadata().rands.clone(),
            time: self.metadata().time as i64,
        }
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
        self.elements.into_iter()
    }
}

impl<'a> IntoIterator for &'a MessageChain {
    type Item = &'a MessageElement;
    type IntoIter = slice::Iter<'a, MessageElement>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.iter()
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
            elements: elems,
            ..Default::default()
        }
    }
}

impl From<ricq::msg::MessageChain> for MessageChain {
    fn from(chain: ricq::msg::MessageChain) -> Self {
        let mut meta = MessageMetadata::default();
        let mut value: Vec<MessageElement> = vec![];

        for val in chain.0 {
            match val {
                MessageElem::AnonGroupMsg(msg) => {
                    let rq = ricq::msg::elem::Anonymous::from(msg);
                    meta.anonymous = Some(Anonymous::from(rq));
                }
                MessageElem::SrcMsg(src) => {
                    let rq = ricq::msg::elem::Reply::from(src);
                    meta.reply = Some(Reply::from(rq));
                }
                or => {
                    let rq = ricq::msg::elem::RQElem::from(or);
                    value.push(MessageElement::from(rq));
                }
            }
        }

        Self {
            meta,
            elements: value,
        }
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

        for value in elem.elements {
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
    Face(Face),
    #[serde(skip)]
    Unknown(RQElem),
}

impl ToString for MessageElement {
    fn to_string(&self) -> String {
        let mut s = String::new();

        match self {
            Self::Text(t) => s.push_str(t),
            Self::Image(img) => {
                let _ = write!(s, "$[Image:{}]", img.url());
            }
            Self::At(At { target, display }) => {
                let _ = write!(s, "$[At:{}({})]", display, target);
            }
            Self::AtAll => s.push_str("$[AtAll]"),
            Self::Face(f) => {
                let _ = write!(s, "$[Face:{}]", f.name);
            }
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
            MessageElement::Face(face) => RQElem::Face(face.into()),
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
            RQElem::Face(face) => MessageElement::Face(face.into()),
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
            Self::Face(face) => PushElem::push_to(face, vec),
            Self::Unknown(_rq) => {}
        }
    }
}
