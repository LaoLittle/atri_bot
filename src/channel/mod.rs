use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use dashmap::mapref::one::Ref;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tracing::{error, warn};

use crate::{Bot, get_app};

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
        let bot: Bot;

        fn get_bot(id: i64) -> Bot {
            get_app().bots.get(&id).expect("Cannot find bot").clone()
        }
        match event {
            QEvent::Login(id) => {
                bot_id = id;
                bot = get_bot(bot_id);

            }
            QEvent::GroupMessage(ref e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin { return; }
                bot = get_bot(bot_id);
            }
            QEvent::GroupAudioMessage(ref e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin { return; }
                bot = get_bot(bot_id);
            }
            QEvent::FriendMessage(ref e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin { return; }
                bot = get_bot(bot_id);
            }
            QEvent::FriendAudioMessage(ref e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin { return; }
                bot = get_bot(bot_id);
            }
            QEvent::GroupTempMessage(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::GroupRequest(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::SelfInvited(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::NewFriendRequest(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::NewMember(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::GroupMute(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::FriendMessageRecall(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::GroupMessageRecall(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::NewFriend(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::GroupLeave(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
                if bot_id == e.inner.member_uin {
                    if let Err(e) = bot.refresh_group_info(e.inner.group_code).await {
                        error!("Error on remove group because group leave: {:?}", e);
                    }
                }

            }
            QEvent::GroupDisband(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
                if let Err(e) = bot.refresh_group_info(e.inner.group_code).await {
                    error!("Error on remove group because group disband: {:?}", e);
                }
            }
            QEvent::FriendPoke(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::GroupNameUpdate(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
                if let Err(e) = bot.refresh_group_info(e.inner.group_code).await {
                    error!("{}: Error on remove group because group name changed: {:?}",bot, e);
                }
            }
            QEvent::DeleteFriend(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::MemberPermissionChange(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
            }
            QEvent::KickedOffline(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
                get_app().bots.remove(&bot_id);
            }
            QEvent::MSFOffline(ref e) => {
                bot_id = e.client.uin().await;
                bot = get_bot(bot_id);
                get_app().bots.remove(&bot_id);
            }
        }



        let _ = global_sender().send(event);
    }
}