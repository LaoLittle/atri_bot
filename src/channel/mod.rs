use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tracing::info;

use crate::contact::member::{AnonymousMember, NamedMember};
use crate::event::{BotLoginEvent, Event, EventInner, FriendMessageEvent, GroupMessageEvent};
use crate::service::listeners::get_global_worker;
use crate::{get_app, get_listener_runtime, unwrap_result_or_print_err_return, Bot};

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

        macro_rules! get_bot {
            ($client:expr) => {
                if let Some(b) = get_app().bots.get(&$client.uin().await).map(|b| b.clone()) {
                    b
                } else {
                    return;
                }
            };
        }

        let self_event = match event {
            QEvent::Login(id) => {
                bot = if let Some(b) = get_bot(id) {
                    b
                } else {
                    return;
                };

                let base = BotLoginEvent::from(bot);
                Event::BotLogin(base)
            }
            QEvent::GroupMessage(e) => {
                fn get_filter_regex() -> &'static Regex {
                    static FILTER_REGEX: OnceLock<Regex> = OnceLock::new();
                    FILTER_REGEX.get_or_init(|| Regex::new("<[$&].+>").expect("Cannot parse regex"))
                }

                bot = get_bot!(e.client);
                let group_id = e.inner.group_code;

                let group_name = || get_filter_regex().replace_all(&e.inner.group_name, "");

                let message = || e.inner.elements.to_string().replace('\n', "\\n");

                if bot.id() == e.inner.from_uin {
                    info!("{bot} >> 群 {}({}): {}", group_name(), group_id, message(),);
                    return;
                }

                let group = if let Some(g) = bot.find_group(group_id) {
                    g
                } else {
                    unwrap_result_or_print_err_return!(bot.refresh_group_info(group_id).await);
                    return;
                };

                let sender = e.inner.from_uin;

                let member: Option<NamedMember>;
                let nick = if sender == AnonymousMember::ID {
                    "匿名"
                } else {
                    member = group.get_named_member(sender).await;
                    member
                        .as_ref()
                        .map(|m| m.nickname())
                        .unwrap_or("NamedMember")
                };

                info!(
                    "{}({}) >> 群 {}({}) >> {bot}: {}",
                    nick,
                    sender,
                    group_name(),
                    group_id,
                    message(),
                );

                let base = GroupMessageEvent::from(group, e);
                Event::GroupMessage(base)
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

                let friend = if let Some(f) = bot.find_friend(e.inner.from_uin) {
                    f
                } else {
                    bot.refresh_friend_list().await.expect("Cannot refresh");
                    return;
                };

                info!(
                    "好友 {}({}) >> {bot}: {}",
                    friend.nickname(),
                    friend.id(),
                    e.inner.elements,
                );

                let base = FriendMessageEvent::from(friend, e);

                Event::FriendMessage(base)
            }
            QEvent::DeleteFriend(e) => {
                bot = get_bot!(e.client);

                bot.delete_friend(e.inner.uin);

                Event::Unknown(EventInner::<QEvent>::from(QEvent::DeleteFriend(e)))
            }
            QEvent::GroupDisband(e) => {
                bot = get_bot!(e.client);

                bot.delete_group(e.inner.group_code);

                Event::Unknown(EventInner::<QEvent>::from(QEvent::GroupDisband(e)))
            }
            QEvent::NewMember(e) => {
                bot = get_bot!(e.client);
                let group_id = e.inner.group_code;
                let member = e.inner.member_uin;

                let group = if let Some(g) = bot.find_group(group_id) {
                    g
                } else {
                    unwrap_result_or_print_err_return!(bot.refresh_group_info(group_id).await);
                    bot.find_group(group_id).unwrap()
                };

                if member == bot.id() {
                } else {
                    let _member = group.get_named_member(e.inner.member_uin).await;
                }

                Event::Unknown(EventInner::<QEvent>::from(QEvent::NewMember(e)))
            }
            QEvent::GroupLeave(e) => {
                bot = get_bot!(e.client);
                let group_id = e.inner.group_code;
                let member = e.inner.member_uin;
                if member == bot.id() {
                    bot.delete_group(group_id);
                } else {
                }

                Event::Unknown(EventInner::<QEvent>::from(QEvent::GroupLeave(e)))
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
