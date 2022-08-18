use crate::message::MessageChain;
use ricq::msg::{MessageElem, PushElem};

pub struct Reply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: MessageChain,
}

impl PushElem for Reply {
    fn push_to(elem: Self, vec: &mut Vec<MessageElem>) {
        let Reply {
            reply_seq,
            sender,
            time,
            elements,
        } = elem;

        let rq = ricq::msg::elem::Reply {
            reply_seq,
            sender,
            time,
            elements: ricq::msg::MessageChain::from(elements),
        };

        let index = if let Some(MessageElem::AnonGroupMsg(..)) = vec.get(0) {
            1
        } else {
            0
        };
        vec.insert(index, rq.into());
    }
}
