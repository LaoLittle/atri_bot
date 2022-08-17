use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::contact::member::Member;
use crate::MessageChain;
use ricq::structs::MessageReceipt;
use ricq::RQResult;
use crate::message::image::Image;

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
    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        match self {
            Self::Friend(f) => f.send_message(chain).await,
            Self::Group(g) => g.send_message(chain).await,
            Self::Member(m) => m.send_message(chain).await,
            Self::Stranger => {
                todo!()
            }
        }
    }

    pub async fn upload_image(&self, chain: MessageChain) -> RQResult<Image> {
        todo!()
    }
}

pub trait HasSubject {
    fn subject(&self) -> Contact;
}
