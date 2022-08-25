use crate::message::{MessageChain, MessageValue};
use ricq::msg::{MessageElem, PushElem};

#[derive(Clone, Default)]
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

#[derive(Default, Debug, Clone)]
pub struct Anonymous {
    pub anon_id: Vec<u8>,
    pub nick: String,
    pub portrait_index: i32,
    pub bubble_index: i32,
    pub expire_time: i32,
    pub color: String,
}

impl From<ricq::msg::elem::Anonymous> for Anonymous {
    fn from(rq: ricq::msg::elem::Anonymous) -> Self {
        let ricq::msg::elem::Anonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = rq;

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
    fn from(ano: Anonymous) -> Self {
        let Anonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = ano;

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

#[derive(Clone)]
pub struct Reply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: Vec<MessageValue>,
}

impl From<ricq::msg::elem::Reply> for Reply {
    fn from(rq: ricq::msg::elem::Reply) -> Self {
        let ricq::msg::elem::Reply {
            reply_seq,
            sender,
            time,
            elements,
        } = rq;

        Self {
            reply_seq,
            sender,
            time,
            elements: MessageChain::from(elements).value,
        }
    }
}

impl From<Reply> for ricq::msg::elem::Reply {
    fn from(reply: Reply) -> Self {
        let Reply {
            reply_seq,
            sender,
            time,
            elements,
        } = reply;

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

        let index = if let Some(MessageElem::AnonGroupMsg(..)) = vec.get(0) {
            1
        } else {
            0
        };
        vec.insert(index, rq.into());
    }
}

pub trait MetaMessage {
    fn metadata(&self) -> &MessageMetadata;
}
