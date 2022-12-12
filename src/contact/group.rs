use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use std::fmt::{Debug, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::error;

use crate::contact::member::NamedMember;
use crate::error::{AtriError, AtriResult};
use crate::message::forward::ForwardMessage;
use crate::message::image::Image;
use crate::message::meta::{MessageReceipt, RecallMessage};
use crate::message::MessageChain;
use crate::Client;

#[derive(Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    #[inline]
    pub fn id(&self) -> i64 {
        self.0.info.code
    }

    #[inline]
    pub fn client(&self) -> &Client {
        &self.0.client
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.0.info.name
    }

    pub async fn members(&self) -> Vec<NamedMember> {
        if self.0.member_list_refreshed.load(Ordering::Relaxed) {
            self.members_cache()
                .iter()
                .filter_map(|named| named.to_owned())
                .collect()
        } else {
            let owner = self.0.info.owner_uin;
            self.client()
                .request_client()
                .get_group_member_list(self.id(), owner)
                .await
                .map(|r| {
                    self.0.member_list_refreshed.store(true, Ordering::Release);
                    r
                })
                .unwrap_or_else(|e| {
                    error!("刷新群聊成员信息时出现错误: {}", e);
                    vec![]
                })
                .into_iter()
                .map(|info| {
                    let named = NamedMember::from(self.clone(), info);
                    self.cache_member(named.clone());
                    named
                })
                .collect()
        }
    }

    pub async fn find_member(&self, id: i64) -> Option<NamedMember> {
        match self.members_cache().entry(id) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let refresh = self.refresh_member(id).await;
                entry.insert(refresh.clone());
                refresh
            }
        }
    }

    async fn _send_message(&self, chain: MessageChain) -> AtriResult<MessageReceipt> {
        self.client()
            .request_client()
            .send_group_message(self.id(), chain.into())
            .await
            .map(MessageReceipt::from)
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

    #[inline]
    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        self._send_message(msg.into()).await
    }

    async fn _send_forward_message(&self, forward: ForwardMessage) -> AtriResult<MessageReceipt> {
        self.client()
            .request_client()
            .send_group_forward_message(self.id(), forward.into())
            .await
            .map(MessageReceipt::from)
            .map_err(AtriError::from)
    }

    #[inline]
    pub async fn send_forward_message<M: Into<ForwardMessage>>(
        &self,
        msg: M,
    ) -> AtriResult<MessageReceipt> {
        self._send_forward_message(msg.into()).await
    }

    /*
    async fn _upload_forward_message(&self, forward: ForwardMessage) -> AtriResult<MessageChain> {
        let t_sum = msgs.len();
        let preview = gen_forward_preview(&msgs);
        let res_id = self.client().request_client().upload_msgs(group_code, msgs, false).await?;
        // TODO friend template?
        let template = format!(
            r##"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><msg serviceID="35" templateID="1" action="viewMultiMsg" brief="[聊天记录]" m_resid="{}" m_fileName="{}" tSum="{}" sourceMsgId="0" url="" flag="3" adverSign="0" multiMsgFlag="0"><item layout="1" advertiser_id="0" aid="0"><title size="34" maxLines="2" lineSpace="12">群聊的聊天记录</title>{}<hr hidden="false" style="0" /><summary size="26" color="#777777">查看{}条转发消息</summary></item><source name="聊天记录" icon="" action="" appid="-1" /></msg>"##,
            res_id,
            std::time::UNIX_EPOCH.elapsed().unwrap_or_else(|| {
                unreachable!()
            }).as_millis(),
            t_sum,
            preview,
            t_sum
        );
        let mut chain = MessageChain::default();
        chain.push(ricq::msg::elem::RichMsg {
            service_id: 35,
            template1: template,
        });
        chain
            .0
            .push(pb::msg::elem::Elem::GeneralFlags(pb::msg::GeneralFlags {
                pendant_id: Some(0),
                pb_reserve: Some(vec![0x78, 0x00, 0xF8, 0x01, 0x00, 0xC8, 0x02, 0x00]),
                ..Default::default()
            }));
        self._send_group_message(group_code, chain.into(), None)
            .await
    }
    */

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

    #[inline]
    pub async fn upload_image<I: Into<Vec<u8>>>(&self, image: I) -> AtriResult<Image> {
        self._upload_image(image.into()).await
    }

    async fn _recall_message(&self, receipt: MessageReceipt) -> AtriResult<()> {
        self.client()
            .request_client()
            .recall_group_message(self.id(), receipt.seqs, receipt.rands)
            .await
            .map_err(AtriError::from)
    }

    #[inline]
    pub async fn recall_message<M: RecallMessage>(&self, msg: &M) -> AtriResult<()> {
        self._recall_message(msg.receipt()).await
    }

    async fn _change_name(&self, name: String) -> AtriResult<()> {
        self.client()
            .request_client()
            .update_group_name(self.id(), name)
            .await
            .map_err(AtriError::from)
    }

    #[inline]
    pub async fn change_name<S: Into<String>>(&self, name: S) -> AtriResult<()> {
        self._change_name(name.into()).await
    }

    pub async fn invite(&self, id: i64) -> AtriResult<()> {
        self.client()
            .request_client()
            .group_invite(self.id(), id)
            .await
            .map_err(AtriError::from)
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

        let deleted = self.client().remove_group_cache(self.id());

        deleted.is_some()
    }
}

// internal impls
impl Group {
    #[inline]
    pub(crate) fn from(bot: Client, info: ricq::structs::GroupInfo) -> Self {
        let imp = imp::Group {
            client: bot,
            info,
            member_list_refreshed: AtomicBool::new(false),
            members: DashMap::new(),
        };

        Self(Arc::new(imp))
    }

    #[inline]
    pub(crate) fn members_cache(&self) -> &DashMap<i64, Option<NamedMember>> {
        &self.0.members
    }

    #[inline]
    pub(crate) fn cache_member(&self, member: NamedMember) {
        self.members_cache().insert(member.id(), Some(member));
    }

    pub(crate) fn remove_member_cache(&self, member_id: i64) -> Option<NamedMember> {
        self.members_cache().remove(&member_id).and_then(|m| m.1)
    }

    pub(crate) async fn try_refresh_member(&self, id: i64) -> AtriResult<Option<NamedMember>> {
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

    pub(crate) async fn refresh_member(&self, id: i64) -> Option<NamedMember> {
        self.try_refresh_member(id)
            .await
            .map_err(|e| {
                error!("刷新成员时出现错误: {}", e);
                e
            })
            .unwrap_or(None)
    }
}

impl Debug for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Group").field(&self.id()).finish()
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "群[{}({})]", self.name(), self.id())
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
        pub members: DashMap<i64, Option<NamedMember>>,
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
