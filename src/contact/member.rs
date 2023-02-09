use crate::contact::group::Group;
use crate::error::{AtriError, AtriResult};
use crate::message::at::At;
use crate::message::meta::{Anonymous, MessageReceipt};
use crate::message::MessageChain;
use crate::{Client, GroupMemberInfo};
use atri_ffi::contact::FFIMember;
use atri_ffi::ffi::ForFFI;
use atri_ffi::ManagedCloneable;
use core::fmt;
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
            Self::Anonymous(_) => AnonymousMember::ID,
        }
    }

    pub fn group(&self) -> &Group {
        match self {
            Self::Named(named) => named.group(),
            Self::Anonymous(ano) => ano.group(),
        }
    }

    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        match self {
            Self::Named(named) => named.send_message(msg).await,
            Self::Anonymous(..) => Err(AtriError::NotSupported),
        }
    }
}

impl ForFFI for Member {
    type FFIValue = FFIMember;

    fn into_ffi(self) -> Self::FFIValue {
        let (is_named, ma) = match self {
            Self::Named(named) => {
                let ma = ManagedCloneable::from_value(named);
                (true, ma)
            }
            Self::Anonymous(ano) => {
                let ma = ManagedCloneable::from_value(ano);
                (false, ma)
            }
        };

        FFIMember {
            is_named,
            inner: ma,
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let ma = value.inner;

        unsafe {
            if value.is_named {
                Self::Named(ma.into_value())
            } else {
                Self::Anonymous(ma.into_value())
            }
        }
    }
}

#[derive(Clone)]
pub struct NamedMember(Arc<imp::NamedMember>);

impl NamedMember {
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

    pub fn client(&self) -> Client {
        self.group().client()
    }

    pub async fn mute(&self, duration: Duration) -> AtriResult<()> {
        self.group()
            .client()
            .request_client()
            .group_mute(self.group().id(), self.id(), duration)
            .await
            .map_err(AtriError::from)
    }

    pub async fn kick<S: AsRef<str>>(&self, msg: Option<S>, block: bool) -> AtriResult<()> {
        let msg = msg.as_ref().map(AsRef::<str>::as_ref).unwrap_or("");

        self.group()
            .client()
            .request_client()
            .group_kick(self.group().id(), vec![self.id()], msg, block)
            .await
            .map_err(AtriError::from)
    }

    pub async fn change_card_name<S: Into<String>>(&self, new: S) -> AtriResult<()> {
        self.group()
            .client()
            .request_client()
            .edit_group_member_card(self.group().id(), self.id(), new.into())
            .await
            .map_err(AtriError::from)
    }

    async fn _send_message(&self, chain: MessageChain) -> AtriResult<MessageReceipt> {
        let client = self.group().client();
        let receipt = if let Some(f) = client.find_friend(self.id()) {
            f.send_message(chain).await?
        } else {
            client
                .request_client()
                .send_group_temp_message(self.group().id(), self.id(), chain.into())
                .await
                .map(MessageReceipt::from)?
        };

        Ok(receipt)
    }

    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        self._send_message(msg.into()).await
    }

    pub fn at(&self) -> At {
        At {
            target: self.id(),
            display: self.card_name().into(),
        }
    }

    pub(crate) fn from(group: Group, info: GroupMemberInfo) -> Self {
        let inner = imp::NamedMember { group, info };

        Self(inner.into())
    }
}

impl fmt::Debug for NamedMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NamedMember").field(&self.id()).finish()
    }
}

impl fmt::Display for NamedMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "群员[{}({})]", self.card_name(), self.id())
    }
}

#[derive(Clone)]
pub struct AnonymousMember(Arc<imp::AnonymousMember>);

impl AnonymousMember {
    pub(crate) fn from(group: Group, info: Anonymous) -> Self {
        let inner = imp::AnonymousMember { group, info };

        Self(inner.into())
    }
}

impl AnonymousMember {
    pub const ID: i64 = 80000000;

    pub fn id(&self) -> &[u8] {
        &self.0.info.anon_id
    }

    pub fn nick(&self) -> &str {
        &self.0.info.nick
    }

    pub fn group(&self) -> &Group {
        &self.0.group
    }

    pub fn client(&self) -> Client {
        self.group().client()
    }
}

mod imp {
    use crate::contact::group::Group;
    use crate::message::meta::Anonymous;
    use crate::GroupMemberInfo;

    pub struct NamedMember {
        pub group: Group,
        pub info: GroupMemberInfo,
    }

    pub struct AnonymousMember {
        pub group: Group,
        pub info: Anonymous,
    }
}
