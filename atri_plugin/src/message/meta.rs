use crate::message::MessageValue;

#[derive(Default)]
pub struct MessageMetadata {
    pub anonymous: Option<Anonymous>,
    pub reply: Option<Reply>,
}

pub struct Reply {
    pub reply_seq: i32,
    pub sender: i64,
    pub time: i32,
    pub elements: Vec<MessageValue>,
}

pub struct Anonymous {
    pub anon_id: Vec<u8>,
    pub nick: String,
    pub portrait_index: i32,
    pub bubble_index: i32,
    pub expire_time: i32,
    pub color: String,
}
