pub mod ffi;
pub mod image;

use crate::Text;
use image::Image;
use ricq::msg::elem::{At, RQElem};
use ricq::msg::{MessageElem, PushElem};

pub struct MessageChain(Vec<MessageValue>);

impl From<ricq::msg::MessageChain> for MessageChain {
    fn from(chain: ricq::msg::MessageChain) -> Self {
        let v: Vec<MessageValue> = chain.into_iter().map(MessageValue::from).collect();
        Self(v)
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
        for value in elem.0 {
            MessageValue::push_to(value, vec);
        }
    }
}

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
            MessageValue::At(at) => RQElem::At(at),
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
            RQElem::At(at) => MessageValue::At(at),
            or => Self::Unknown(or),
        }
    }
}

impl PushElem for MessageValue {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        match elem {
            Self::Text(s) => PushElem::push_to(Text::new(s), vec),
            Self::Image(img) => PushElem::push_to(img, vec),
            _ => {}
        }
    }
}
