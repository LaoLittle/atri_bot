use crate::message::{MessageChain, MessageElement};
use ricq::msg::{MessageElem, PushElem};
use serde::{Deserialize, Serialize};

pub trait MetaMessage {
    fn metadata(&self) -> &MessageMetadata;
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MessageMetadata {
    pub seqs: Vec<i32>,
    pub rands: Vec<i32>,
    pub time: i32,
    pub sender: i64,
    pub anonymous: Option<Anonymous>,
    pub reply: Option<Reply>,
}

impl MetaMessage for MessageMetadata {
    fn metadata(&self) -> &MessageMetadata {
        self
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Anonymous {
    pub anon_id: Vec<u8>,
    pub nick: String,
    pub portrait_index: i32,
    pub bubble_index: i32,
    pub expire_time: i32,
    pub color: String,
}

impl From<ricq::msg::elem::Anonymous> for Anonymous {
    fn from(
        ricq::msg::elem::Anonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }: ricq::msg::elem::Anonymous,
    ) -> Self {
        Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }
    }
}

impl From<Anonymous> for ricq::msg::elem::Anonymous {
    fn from(
        Anonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }: Anonymous,
    ) -> Self {
        Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }
    }
}

impl PushElem for Anonymous {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let rq = ricq::msg::elem::Anonymous::from(elem);

        vec.insert(0, rq.into());
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Reply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: Vec<MessageElement>,
}

impl From<ricq::msg::elem::Reply> for Reply {
    fn from(
        ricq::msg::elem::Reply {
            reply_seq,
            sender,
            time,
            elements,
        }: ricq::msg::elem::Reply,
    ) -> Self {
        Self {
            reply_seq,
            sender,
            time,
            elements: MessageChain::from(elements).elements,
        }
    }
}

impl From<Reply> for ricq::msg::elem::Reply {
    fn from(
        Reply {
            reply_seq,
            sender,
            time,
            elements,
        }: Reply,
    ) -> Self {
        Self {
            reply_seq,
            sender,
            time,
            elements: ricq::msg::MessageChain::from(MessageChain::from(elements)),
        }
    }
}

impl PushElem for Reply {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let rq = ricq::msg::elem::Reply::from(elem);

        let index = match vec.get(0) {
            Some(MessageElem::AnonGroupMsg(..)) => 1,
            _ => 0,
        };

        vec.insert(index, rq.into());
    }
}
