use std::sync::Arc;
use ricq::RQResult;
use ricq::structs::{FriendInfo, MessageReceipt};
use tracing::error;
use crate::{Bot, MessageChain};
use crate::message::Image;

#[derive(Clone)]
pub struct Friend(Arc<imp::Friend>);

impl Friend {
    pub fn id(&self) -> i64 {
        self.0.info.uin
    }

    pub fn nickname(&self) -> &str {
        &self.0.info.nick
    }

    pub fn remark(&self) -> &str {
        &self.0.info.remark
    }

    pub fn bot(&self) -> &Bot {
        &self.0.bot
    }

    pub async fn delete(&self) -> bool {
        let result = self.bot()
            .client()
            .delete_friend(self.id())
            .await;

        if let Err(e) = result {
            error!("尝试删除好友 {}({}) 时失败: {:?}",self.nickname(), self.id(), e);
            return false;
        }

        let map = self.bot().delete_friend(self.id());

        map.is_some()
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        self.bot()
            .client()
            .send_friend_message(self.id(), chain)
            .await
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<Image> {
        self.bot()
            .client()
            .upload_friend_image(self.id(), image)
            .await
            .map(|f| Image::Friend(f))
    }

    pub(crate) fn from(bot: Bot,info: FriendInfo) -> Self {
        let f = imp::Friend {
            bot,
            info
        };

        Self(Arc::new(f))
    }
}

mod imp {
    use ricq::structs::FriendInfo;
    use crate::Bot;

    pub struct Friend {
        pub bot: Bot,
        pub info: FriendInfo,
    }
}