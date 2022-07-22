use std::fmt::{Display, Formatter};
use std::sync::Arc;

use ricq::msg::elem::GroupImage;
use ricq::RQResult;
use ricq::structs::{GroupInfo, MessageReceipt};

use crate::{Bot, MessageChain};

#[derive(Debug, Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    pub fn from(bot: Bot, info: GroupInfo) -> Self {
        let imp = imp::Group {
            id: info.code,
            bot,
            info,
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

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        self.bot().client().send_group_message(
            self.id(),
            chain,
        ).await
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<GroupImage> {
        self.bot().client().upload_group_image(
            self.id(),
            image,
        ).await
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id())
    }
}

mod imp {
    use ricq::structs::GroupInfo;

    use crate::Bot;

    #[derive(Debug)]
    pub struct Group {
        pub id: i64,
        pub bot: Bot,
        pub info: GroupInfo,
    }

    pub struct NamedMember;

    pub struct AnonymousMember;
}