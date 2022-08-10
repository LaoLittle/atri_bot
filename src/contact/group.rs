use std::fmt::{Display, Formatter};
use std::sync::Arc;
use dashmap::DashMap;

use ricq::msg::elem::GroupImage;
use ricq::RQResult;
use ricq::structs::{GroupInfo, MessageReceipt};
use tracing::error;

use crate::{Bot, GroupMemberInfo, MessageChain};

#[derive(Debug, Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    pub fn from(bot: Bot, info: GroupInfo) -> Self {
        let imp = imp::Group {
            id: info.code,
            bot,
            info,
            members: DashMap::new()
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
    
    pub async fn find_member(&self, id: i64) -> Option<Arc<GroupMemberInfo>> {
        if let Some(info) = self.0.members.get(&id) {
            return Some(info.clone());
        }

        let result = self.bot().client().get_group_member_info(self.id(), id).await;
        if let Ok(info) = result {
            let arc = Arc::new(info);
            self.0.members.insert(id, arc.clone());
            Some(arc)
        } else { None }
    }
    
    pub(crate) fn insert_member(&self, info: GroupMemberInfo) {
        let arc = Arc::new(info);
        
        self.0.members.insert(arc.uin, arc);
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        let result = self.bot().client().send_group_message(
            self.id(),
            chain,
        ).await;
        
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
        let result = self.bot().client().upload_group_image(
            self.id(),
            image,
        ).await;

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
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id())
    }
}

mod imp {
    use std::sync::Arc;
    use dashmap::DashMap;
    use ricq::structs::GroupInfo;

    use crate::{Bot, GroupMemberInfo};

    #[derive(Debug)]
    pub struct Group {
        pub id: i64,
        pub bot: Bot,
        pub info: GroupInfo,
        pub members: DashMap<i64, Arc<GroupMemberInfo>>
    }

    pub struct NamedMember;

    pub struct AnonymousMember;
}