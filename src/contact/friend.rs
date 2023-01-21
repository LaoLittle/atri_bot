use crate::error::{AtriError, AtriResult};
use crate::message::forward::ForwardMessage;
use crate::message::image::Image;
use crate::message::meta::{MessageReceipt, RecallMessage};
use crate::message::MessageChain;
use crate::Client;
use std::fmt;
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

        let deleted = self.client().remove_friend_cache(self.id());

        deleted.is_some()
    }

    async fn _send_message(&self, chain: MessageChain) -> AtriResult<MessageReceipt> {
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

        result.map(MessageReceipt::from).map_err(AtriError::from)
    }

    pub async fn send_message<M: Into<MessageChain>>(&self, msg: M) -> AtriResult<MessageReceipt> {
        self._send_message(msg.into()).await
    }

    async fn _send_forward_message(&self, _forward: ForwardMessage) -> AtriResult<MessageReceipt> {
        Err(AtriError::NotSupported)
    }

    pub async fn _upload_image(&self, image: &[u8]) -> AtriResult<Image> {
        let f = self
            .client()
            .request_client()
            .upload_friend_image(self.id(), image)
            .await?;

        Ok(Image::Friend(f))
    }

    pub async fn upload_image<B: AsRef<[u8]>>(&self, image: B) -> AtriResult<Image> {
        self._upload_image(image.as_ref()).await
    }

    async fn _recall_message(&self, receipt: MessageReceipt) -> AtriResult<()> {
        self.client()
            .request_client()
            .recall_friend_message(
                self.id(),
                receipt.time,
                receipt.seqs.clone(),
                receipt.rands.clone(),
            )
            .await
            .map_err(AtriError::from)
    }

    pub async fn recall_message<M: RecallMessage>(&self, msg: &M) -> AtriResult<()> {
        self._recall_message(msg.receipt()).await
    }
}

// internal impls
impl Friend {
    pub(crate) fn from(client: Client, info: ricq::structs::FriendInfo) -> Self {
        let f = imp::Friend { client, info };

        Self(Arc::new(f))
    }
}

impl fmt::Debug for Friend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Friend").field(&self.id()).finish()
    }
}

impl fmt::Display for Friend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "好友[{}({})]", self.nickname(), self.id())
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
