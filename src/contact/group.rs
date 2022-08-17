use dashmap::DashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use ricq::msg::elem::GroupImage;
use ricq::structs::{GroupInfo, MessageReceipt};
use ricq::RQResult;
use tracing::error;

use crate::contact::member::NamedMember;
use crate::{Bot, MessageChain};
use crate::message::Image;

#[derive(Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    pub fn from(bot: Bot, info: GroupInfo) -> Self {
        let imp = imp::Group {
            id: info.code,
            bot,
            info,
            members: DashMap::new(),
        };

        Self(Arc::new(imp))
    }

    pub fn id(&self) -> i64 {
        self.0.id
    }

    pub fn bot(&self) -> &Bot {
        &self.0.bot
    }

    pub fn name(&self) -> &str {
        &self.0.info.name
    }

    pub async fn find_member(&self, id: i64) -> Option<NamedMember> {
        if let Some(member) = self.0.members.get(&id) {
            return Some(member.clone());
        }

        self.bot()
            .client()
            .get_group_member_info(self.id(), id)
            .await
            .ok()
            .map(|info| NamedMember::from(self.clone(), info))
            .map(|member| {
                self.0.members.insert(id, member.clone());
                member
            })
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        self.bot()
            .client()
            .send_group_message(self.id(), chain)
            .await
            .map_err(|err| {
                error!(
                    "{}发送信息失败, 目标群: {}({}), {:?}",
                    self.bot(),
                    self.name(),
                    self.id(),
                    err
                );
                err
            })
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<Image> {
        self.bot()
            .client()
            .upload_group_image(self.id(), image)
            .await
            .map(|g| Image::Group(g))
            .map_err(|err| {
                error!(
                    "{}上传图片失败, 目标群: {}({}), {:?}",
                    self.bot(),
                    self.name(),
                    self.id(),
                    err
                );
                err
            })
    }

    pub async fn quit(&self) -> bool {
        let result = self.bot().client().group_quit(self.id()).await;
        if let Err(e) = result {
            error!("尝试退出群 {}({}) 时失败: {:?}", self.name(), self.id(), e);
            return false;
        }

        let map = self.bot().delete_group(self.id());

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

    use crate::contact::member::NamedMember;
    use crate::Bot;

    pub struct Group {
        pub id: i64,
        pub bot: Bot,
        pub info: GroupInfo,
        pub members: DashMap<i64, NamedMember>,
    }
}
