use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};

use crate::Bot;

static GLOBAL_EVENT_CHANNEL: OnceLock<Sender<QEvent>> = OnceLock::<Sender<QEvent>>::new();

pub fn global_sender() -> &'static Sender<QEvent> {
    GLOBAL_EVENT_CHANNEL.get_or_init(|| {
        let channel = channel(128);

        channel.0
    })
}

pub fn global_receiver() -> Receiver<QEvent> {
    global_sender().subscribe()
}

pub struct GlobalEventBroadcastHandler;

#[async_trait]
impl ricq::handler::Handler for GlobalEventBroadcastHandler {
    async fn handle(&self, event: QEvent) {
        let bot_id: i64;
        let bot: Arc<Bot>;

        match event {
            QEvent::Login(id) => {
                bot_id = id;
            }
            QEvent::GroupMessage(ref e) => {
                bot_id = e.client.uin().await;
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            QEvent::GroupAudioMessage(ref e) => {
                bot_id = e.client.uin().await;
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            QEvent::FriendMessage(ref e) => {
                bot_id = e.client.uin().await;
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            QEvent::FriendAudioMessage(ref e) => {
                bot_id = e.client.uin().await;
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            QEvent::GroupTempMessage(ref e) => {
                bot_id = e.client.uin().await;
            }
            QEvent::GroupRequest(ref e) => {
                bot_id = e.client.uin().await;
            }
            QEvent::SelfInvited(ref e) => {
                bot_id = e.client.uin().await;
            }

            _ => {}
        }

        let _ = global_sender().send(event);
    }
}