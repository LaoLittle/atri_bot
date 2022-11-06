use crate::message::image::Image;
use crate::message::meta::MetaMessage;
use crate::message::MessageChain;
use crate::Client;
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

    pub fn client(&self) -> &Client {
        &self.0.client
    }

    pub async fn delete(&self) -> bool {
        let result = self
            .client()
            .request_client()
            .delete_friend(self.id())
            .await;

        if let Err(e) = result {
            error!(
                "尝试删除好友 {}({}) 时失败: {:?}",
                self.nickname(),
                self.id(),
                e
            );
            return false;
        }

        let map = self.client().remove_friend_cache(self.id());

        map.is_some()
    }

    pub async fn send_message(&self, chain: MessageChain) -> RQResult<MessageReceipt> {
        let result = self
            .client()
            .request_client()
            .send_friend_message(self.id(), chain.into())
            .await;

        if let Err(ref e) = result {
            error!(
                "{}发送消息失败, 目标好友: {}({}), {:?}",
                self.client(),
                self.nickname(),
                self.id(),
                e
            );
        }

        result
    }

    pub async fn upload_image(&self, image: Vec<u8>) -> RQResult<Image> {
        self.client()
            .request_client()
            .upload_friend_image(self.id(), image)
            .await
            .map(Image::Friend)
    }

    pub async fn recall_message<M: MetaMessage>(&self, msg: &M) -> RQResult<()> {
        let meta = msg.metadata();
        self.client()
            .request_client()
            .recall_friend_message(
                self.id(),
                meta.time as i64,
                meta.seqs.clone(),
                meta.rands.clone(),
            )
            .await
    }

    pub(crate) fn from(client: Client, info: FriendInfo) -> Self {
        let f = imp::Friend { client, info };

        Self(Arc::new(f))
    }
}

mod imp {
    use crate::Client;
    use ricq::structs::FriendInfo;

    pub struct Friend {
        pub client: Client,
        pub info: FriendInfo,
    }
}
