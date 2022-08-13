use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tracing::info;

use crate::event::{BotOnlineEvent, Event, EventInner, GroupMessageEvent};
use crate::service::listeners::get_global_worker;
use crate::{get_app, get_listener_runtime, Bot};

static GLOBAL_EVENT_CHANNEL: OnceLock<Sender<Event>> = OnceLock::<Sender<Event>>::new();

pub fn global_sender() -> &'static Sender<Event> {
    GLOBAL_EVENT_CHANNEL.get_or_init(|| {
        let channel = channel(128);

        channel.0
    })
}

pub fn global_receiver() -> Receiver<Event> {
    global_sender().subscribe()
}

pub struct GlobalEventBroadcastHandler;

#[async_trait]
impl ricq::handler::Handler for GlobalEventBroadcastHandler {
    async fn handle(&self, event: QEvent) {
        let bot_id: i64;
        let bot: Bot;

        fn get_bot(id: i64) -> Option<Bot> {
            get_app().bots.get(&id).map(|b| b.clone())
        }

        let self_event = match event {
            QEvent::Login(id) => {
                bot_id = id;
                bot = if let Some(b) = get_bot(bot_id) {
                    b
                } else {
                    return;
                };

                let base = BotOnlineEvent::from(bot);
                let inner = Event::BotOnlineEvent(base);
                inner.into()
            }
            QEvent::GroupMessage(e) => {
                static FILTER_REGEX: OnceLock<Regex> = OnceLock::new();

                fn get_filter_regex() -> &'static Regex {
                    FILTER_REGEX.get_or_init(|| Regex::new("<[$&].+>").expect("Cannot parse regex"))
                }

                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin {
                    return;
                }
                bot = if let Some(b) = get_bot(bot_id) {
                    b
                } else {
                    return;
                };

                let group = bot.find_group(e.inner.group_code).await.unwrap();

                let filter = get_filter_regex();

                info!(
                    "群 {}({}) >> {bot}: {}",
                    filter.replace_all(group.name(), ""),
                    group.id(),
                    e.inner.elements,
                );

                let base = GroupMessageEvent::from(group, e);
                Event::GroupMessageEvent(base)
            }
            QEvent::FriendMessage(e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin {
                    return;
                }
                bot = if let Some(b) = get_bot(bot_id) {
                    b
                } else {
                    return;
                };

                info!(
                    "好友 {}({}) >> {bot}: {}",
                    e.inner.from_uin, e.inner.from_nick, e.inner.elements,
                );

                Event::Unknown(EventInner::<QEvent>::from(QEvent::FriendMessage(e)))
            }
            or => Event::Unknown(EventInner::<QEvent>::from(or)),
        };

        let e = self_event.clone();
        get_listener_runtime().spawn(async move {
            get_global_worker().handle(&e).await;
        });

        let _ = global_sender().send(self_event);
    }
}
