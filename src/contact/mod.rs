use crate::contact::group::Group;
use crate::MessageChain;

pub mod group;

pub enum Contact {
    Friend,
    Group(Group),
    Stranger,
}

impl Contact {
    pub async fn send_message(&self, chain: MessageChain) {
        match self {
            Self::Friend => {}
            Self::Group(g) => {
                g.send_message(chain).await.ok();
            }
            Self::Stranger => {}
        }
    }
}

pub trait HasSubject {
    fn subject(&self) -> Contact;
}