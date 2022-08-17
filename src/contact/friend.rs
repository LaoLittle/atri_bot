use crate::message::image::Image;
use crate::{Bot, MessageChain};
use ricq::structs::{FriendInfo, MessageReceipt};
use ricq::RQResult;
use std::sync::Arc;
use tracing::error;

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
        let result = self.bot().client().delete_friend(self.id()).await;

        if let Err(e) = result {
            error!(
                "尝试删除好友 {}({}) 时失败: {:?}",
                self.nickname(),
                self.id(),
                e
            );
            return false;
        }

        let map = self.bot().delete_friend(self.id());

        map.is_some()
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        let result = self
            .bot()
            .client()
            .send_friend_message(self.id(), chain)
            .await;

        if let Err(ref e) = result {
            error!(
                "{}发送消息失败, 目标好友: {}({}), {:?}",
                self.bot(),
                self.nickname(),
                self.id(),
                e
            );
        }

        result
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<Image> {
        self.bot()
            .client()
            .upload_friend_image(self.id(), image)
            .await
            .map(|f| Image::Friend(f))
    }

    pub(crate) fn from(bot: Bot, info: FriendInfo) -> Self {
        let f = imp::Friend { bot, info };

        Self(Arc::new(f))
    }
}

mod imp {
    use crate::Bot;
    use ricq::structs::FriendInfo;

    pub struct Friend {
        pub bot: Bot,
        pub info: FriendInfo,
    }
}
