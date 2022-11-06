use dashmap::DashMap;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ricq::structs::{GroupInfo, MessageReceipt};
use ricq::RQResult;
use tracing::error;

use crate::contact::member::NamedMember;
use crate::message::image::Image;
use crate::message::meta::MetaMessage;
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

    pub fn find_member(&self, id: i64) -> Option<NamedMember> {
        if let Some(member) = self.0.members.get(&id) {
            return Some(member.clone());
        }

        None
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

    pub async fn get_named_member(&self, id: i64) -> Option<NamedMember> {
        if let Some(named) = self.find_member(id) {
            return Some(named);
        }

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
                self.0.members.insert(id, named.clone());
                Some(named)
            });

        named.unwrap_or(None)
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
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
                err
            })
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<Image> {
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
                err
            })
    }

    pub async fn recall_message<M: MetaMessage>(&self, msg: &M) -> RQResult<()> {
        let meta = msg.metadata();
        self.client()
            .request_client()
            .recall_group_message(self.id(), meta.seqs.clone(), meta.rands.clone())
            .await
    }

    pub async fn change_name(&self, name: String) -> RQResult<()> {
        self.client()
            .request_client()
            .update_group_name(self.id(), name)
            .await
    }

    pub async fn kick<M: ToKickMember, S: AsRef<str>>(
        &self,
        member: M,
        msg: Option<S>,
        block: bool,
    ) -> RQResult<()> {
        let members = member.to_member_vec();
        let msg = msg.as_ref().map(AsRef::<str>::as_ref).unwrap_or("");

        self.client()
            .request_client()
            .group_kick(self.id(), members, msg, block)
            .await
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
