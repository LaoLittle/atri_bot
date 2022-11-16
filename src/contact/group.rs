use dashmap::DashMap;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ricq::structs::{GroupInfo, MessageReceipt};
use tracing::error;

use crate::contact::member::NamedMember;
use crate::error::{AtriError, AtriResult};
use crate::message::image::Image;
use crate::message::meta::{MessageMetadata, MetaMessage};
use crate::message::MessageChain;
use crate::Client;

#[derive(Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    pub fn from(bot: Client, info: GroupInfo) -> Self {
        let imp = imp::Group {
            client: bot,
            info,
            member_list_refreshed: AtomicBool::new(false),
            members: DashMap::new(),
        };

        Self(Arc::new(imp))
    }

    pub fn id(&self) -> i64 {
        self.0.info.code
    }

    pub fn client(&self) -> &Client {
        &self.0.client
    }

    pub fn name(&self) -> &str {
        &self.0.info.name
    }

    pub async fn members(&self) -> Vec<NamedMember> {
        if self.0.member_list_refreshed.load(Ordering::Relaxed) {
            self.0.members.iter().map(|named| named.clone()).collect()
        } else {
            let owner = self.0.info.owner_uin;
            self.client()
                .request_client()
                .get_group_member_list(self.id(), owner)
                .await
                .map(|r| {
                    self.0.member_list_refreshed.swap(true, Ordering::Release);
                    r
                })
                .unwrap_or_else(|e| {
                    error!("刷新群聊成员信息时出现错误: {:?}", e);
                    vec![]
                })
                .into_iter()
                .map(|info| {
                    let named = NamedMember::from(self.clone(), info);
                    self.0.members.insert(named.id(), named.clone());
                    named
                })
                .collect()
        }
    }

    pub fn find_member(&self, id: i64) -> Option<NamedMember> {
        if let Some(member) = self.0.members.get(&id) {
            return Some(member.clone());
        }

        None
    }

    pub async fn try_refresh_member(&self, id: i64) -> AtriResult<Option<NamedMember>> {
        let named = self
            .client()
            .request_client()
            .get_group_member_info(self.id(), id)
            .await
            .map(|info| {
                if info.join_time == 0 {
                    return None;
                }

                let named = NamedMember::from(self.clone(), info);
                self.cache_member(named.clone());
                Some(named)
            })?;

        Ok(named)
    }

    pub async fn refresh_member(&self, id: i64) -> Option<NamedMember> {
        self.try_refresh_member(id).await.unwrap_or(None)
    }

    pub async fn find_or_refresh_member(&self, id: i64) -> Option<NamedMember> {
        if let Some(named) = self.find_member(id) {
            return Some(named);
        }

        self.refresh_member(id).await
    }

    async fn _send_message(&self, chain: MessageChain) -> AtriResult<MessageReceipt> {
        self.client()
            .request_client()
            .send_group_message(self.id(), chain.into())
            .await
            .map_err(|err| {
                error!(
                    "{}发送信息失败, 目标群: {}({}), {:?}",
                    self.client(),
                    self.name(),
                    self.id(),
                    err
                );

                AtriError::from(err)
            })
    }

    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        self._send_message(msg.into()).await
    }

    async fn _upload_image(&self, image: Vec<u8>) -> AtriResult<Image> {
        self.client()
            .request_client()
            .upload_group_image(self.id(), image)
            .await
            .map(Image::Group)
            .map_err(|err| {
                error!(
                    "{}上传图片失败, 目标群: {}({}), {:?}",
                    self.client(),
                    self.name(),
                    self.id(),
                    err
                );

                AtriError::from(err)
            })
    }

    pub async fn upload_image<I: Into<Vec<u8>>>(&self, image: I) -> AtriResult<Image> {
        self._upload_image(image.into()).await
    }

    async fn _recall_message(&self, meta: &MessageMetadata) -> AtriResult<()> {
        self.client()
            .request_client()
            .recall_group_message(self.id(), meta.seqs.clone(), meta.rands.clone())
            .await
            .map_err(AtriError::from)
    }

    pub async fn recall_message<M: MetaMessage>(&self, msg: &M) -> AtriResult<()> {
        self._recall_message(msg.metadata()).await
    }

    async fn _change_name(&self, name: String) -> AtriResult<()> {
        self.client()
            .request_client()
            .update_group_name(self.id(), name)
            .await
            .map_err(AtriError::from)
    }

    pub async fn change_name<S: Into<String>>(&self, name: S) -> AtriResult<()> {
        self._change_name(name.into()).await
    }

    pub async fn kick<M: ToKickMember, S: AsRef<str>>(
        &self,
        member: M,
        msg: Option<S>,
        block: bool,
    ) -> AtriResult<()> {
        let members = member.to_member_vec();
        let msg = msg.as_ref().map(AsRef::<str>::as_ref).unwrap_or("");

        self.client()
            .request_client()
            .group_kick(self.id(), members, msg, block)
            .await
            .map_err(AtriError::from)
    }

    pub async fn quit(&self) -> bool {
        let result = self.client().request_client().group_quit(self.id()).await;
        if let Err(e) = result {
            error!("尝试退出群 {}({}) 时失败: {:?}", self.name(), self.id(), e);
            return false;
        }

        let map = self.client().remove_group_cache(self.id());

        map.is_some()
    }

    pub(crate) fn cache_member(&self, member: NamedMember) {
        self.0.members.insert(member.id(), member);
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id(),)
    }
}

mod imp {
    use dashmap::DashMap;
    use ricq::structs::GroupInfo;
    use std::sync::atomic::AtomicBool;

    use crate::contact::member::NamedMember;
    use crate::Client;

    pub struct Group {
        pub client: Client,
        pub info: GroupInfo,
        pub member_list_refreshed: AtomicBool,
        pub members: DashMap<i64, NamedMember>,
    }
}

pub trait ToKickMember {
    fn to_member_vec(self) -> Vec<i64>;
}

impl ToKickMember for i64 {
    fn to_member_vec(self) -> Vec<i64> {
        vec![self]
    }
}

impl ToKickMember for Vec<i64> {
    fn to_member_vec(self) -> Vec<i64> {
        self
    }
}

impl<const N: usize> ToKickMember for [i64; N] {
    fn to_member_vec(self) -> Vec<i64> {
        self.to_vec()
    }
}

impl<const N: usize> ToKickMember for &[i64; N] {
    fn to_member_vec(self) -> Vec<i64> {
        self.to_vec()
    }
}
