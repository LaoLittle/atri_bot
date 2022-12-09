use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::contact::member::Member;
use crate::error::{AtriError, AtriResult};
use crate::message::meta::MessageReceipt;
use crate::message::MessageChain;

pub mod friend;
pub mod group;
pub mod member;

pub enum Contact {
    Friend(Friend),
    Group(Group),
    Member(Member),
    Stranger,
}

impl Contact {
    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        match self {
            Self::Friend(f) => f.send_message(msg).await,
            Self::Group(g) => g.send_message(msg).await,
            Self::Member(m) => m.send_message(msg).await,
            Self::Stranger => Err(AtriError::NotSupported), // todo
        }
    }
}

pub trait ContactSubject {
    fn subject(&self) -> Contact;
}
