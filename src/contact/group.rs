use dashmap::DashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use ricq::msg::elem::GroupImage;
use ricq::structs::{GroupInfo, MessageReceipt};
use ricq::RQResult;
use tracing::error;

use crate::contact::member::NamedMember;
use crate::{Bot, MessageChain};

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

        let result = self
            .bot()
            .client()
            .get_group_member_info(self.id(), id)
            .await;
        if let Ok(info) = result {
            let member = NamedMember::from(self.clone(), info);
            self.0.members.insert(id, member.clone());
            Some(member)
        } else {
            None
        }
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        let result = self
            .bot()
            .client()
            .send_group_message(self.id(), chain)
            .await;

        if let Err(ref err) = result {
            error!(
                "{}发送信息失败, 目标群: {}({}), {:?}",
                self.bot(),
                self.name(),
                self.id(),
                err
            )
        }

        result
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<GroupImage> {
        let result = self
            .bot()
            .client()
            .upload_group_image(self.id(), image)
            .await;

        if let Err(ref err) = result {
            error!(
                "{}上传图片失败, 目标群: {}({}), {:?}",
                self.bot(),
                self.name(),
                self.id(),
                err
            )
        }

        result
    }

    pub async fn quit(&self) {
        let _ = self.bot().client().group_quit(self.id()).await;
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id())
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
