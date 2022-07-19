use std::sync::OnceLock;

use async_trait::async_trait;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};

use crate::{Bot, get_app};
use crate::event::{BotOnlineEvent, Event, GroupMessageEvent};

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

        let _event_: Event;
        fn get_bot(id: i64) -> Bot {
            get_app().bots.get(&id).expect("Cannot find bot").clone()
        }
        match event {
            QEvent::Login(id) => {
                bot_id = id;
                bot = get_bot(bot_id);

                let base = BotOnlineEvent::from(bot);
                let inner = Event::BotOnlineEvent(base);
                _event_ = inner.into();
            }
            QEvent::GroupMessage(e) => {
                bot_id = e.client.uin().await;
                if bot_id == e.inner.from_uin { return; }
                bot = get_bot(bot_id);
                let base = GroupMessageEvent::from(
                    bot.find_group(e.inner.group_code).unwrap(),
                    e,
                );
                _event_ = Event::GroupMessageEvent(base);
            }
            or => {
                _event_ = Event::Unknown(or);
            }
        }

        let _ = global_sender().send(_event_);
    }
}