use crate::contact::group::Group;
use crate::{GroupMemberInfo, MessageChain};
use ricq::structs::MessageReceipt;
use ricq::{RQError, RQResult};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub enum Member {
    Named(NamedMember),
    Anonymous(AnonymousMember),
}

impl Member {
    pub fn id(&self) -> i64 {
        match self {
            Self::Named(named) => named.id(),
            Self::Anonymous(an) => an.id(),
        }
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        match self {
            Self::Named(named) => named.send_message(chain).await,
            Self::Anonymous(..) => Err(RQError::Other("".into())),
        }
    }
}

#[derive(Clone)]
pub struct NamedMember(Arc<imp::NamedMember>);

impl NamedMember {
    pub(crate) fn from(group: Group, info: GroupMemberInfo) -> Self {
        let inner = imp::NamedMember { group, info };

        Self(inner.into())
    }

    pub fn id(&self) -> i64 {
        self.0.info.uin
    }

    pub fn nickname(&self) -> &str {
        &self.0.info.nickname
    }

    pub fn card_name(&self) -> &str {
        &self.0.info.card_name
    }

    pub fn group(&self) -> &Group {
        &self.0.group
    }

    pub async fn mute(&self, duration: Duration) -> RQResult<()> {
        self.group()
            .bot()
            .client()
            .group_mute(self.group().id(), self.id(), duration)
            .await
    }

    pub async fn kick<M: KickMessage>(&self, msg: M) -> RQResult<()> {
        let (msg, block) = msg.to_kick_message();
        self.group()
            .bot()
            .client()
            .group_kick(self.group().id(), vec![self.id()], msg, block)
            .await
    }

    pub async fn change_card_name<S: ToString>(&self, new: S) -> RQResult<()> {
        self.group()
            .bot()
            .client()
            .edit_group_member_card(self.group().id(), self.id(), new.to_string())
            .await
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        let bot = self.group().bot();
        if let Some(f) = bot.find_friend(self.id()).await {
            f.send_message(chain).await
        } else {
            bot.client()
                .send_group_temp_message(self.group().id(), self.id(), chain)
                .await
        }
    }
}

pub trait KickMessage: Sized {
    fn to_kick_message(&self) -> (&str, bool);
}

impl KickMessage for bool {
    fn to_kick_message(&self) -> (&str, bool) {
        ("", *self)
    }
}

impl<S: AsRef<str>> KickMessage for (S, bool) {
    fn to_kick_message(&self) -> (&str, bool) {
        (self.0.as_ref(), self.1)
    }
}

#[derive(Clone)]
pub struct AnonymousMember(Arc<imp::AnonymousMember>);

impl AnonymousMember {
    pub(crate) fn from(group: Group, id: i64) -> Self {
        let inner = imp::AnonymousMember { id, group };

        Self(inner.into())
    }
}

impl AnonymousMember {
    pub fn id(&self) -> i64 {
        self.0.id
    }

    pub fn group(&self) -> &Group {
        &self.0.group
    }

    pub async fn mute(&self, duration: Duration) -> RQResult<()> {
        self.group()
            .bot()
            .client()
            .group_mute(self.group().id(), self.id(), duration)
            .await
    }
}

mod imp {
    use crate::contact::group::Group;
    use crate::GroupMemberInfo;

    pub struct NamedMember {
        pub group: Group,
        pub info: GroupMemberInfo,
    }

    pub struct AnonymousMember {
        pub group: Group,
        pub id: i64,
    }
}
