use crate::message::MessageChain;

pub struct Reply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: MessageChain,
}