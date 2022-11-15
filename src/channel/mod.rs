use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tracing::{error, info, warn};

use crate::contact::member::{AnonymousMember, NamedMember};
use crate::event::{ClientLoginEvent, Event, EventInner, FriendMessageEvent, GroupMessageEvent};
use crate::get_global_listener_worker;
use crate::{get_listener_runtime, global_status, unwrap_result_or_print_err_return, Client};

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
        let client: Client;

        fn get_bot(id: i64) -> Option<Client> {
            global_status().clients.get(&id).map(|b| b.clone())
        }

        macro_rules! get_client {
            ($client:expr) => {
                if let Some(b) = global_status()
                    .clients
                    .get(&$client.uin().await)
                    .map(|b| b.clone())
                {
                    b
                } else {
                    return;
                }
            };
        }

        let self_event = match event {
            QEvent::Login(id) => {
                client = if let Some(b) = get_bot(id) {
                    b
                } else {
                    return;
                };

                let base = ClientLoginEvent::from(client);
                Event::ClientLogin(base)
            }
            QEvent::GroupMessage(e) => {
                fn get_filter_regex() -> &'static Regex {
                    static FILTER_REGEX: OnceLock<Regex> = OnceLock::new();
                    FILTER_REGEX.get_or_init(|| Regex::new("<[$&].+>").expect("Cannot parse regex"))
                }

                client = get_client!(e.client);
                let group_id = e.inner.group_code;

                let group_name = || get_filter_regex().replace_all(&e.inner.group_name, "");

                let message = || e.inner.elements.to_string().replace('\n', "\\n");

                if client.id() == e.inner.from_uin {
                    info!(
                        "{client} >> 群 {}({}): {}",
                        group_name(),
                        group_id,
                        message(),
                    );
                    return;
                }

                let group = if let Some(g) = client.find_group(group_id) {
                    g
                } else {
                    unwrap_result_or_print_err_return!(client.refresh_group_info(group_id).await);
                    return;
                };

                let sender = e.inner.from_uin;

                let member: Option<NamedMember>;
                let nick = if sender == AnonymousMember::ID {
                    "匿名"
                } else {
                    member = group
                        .try_get_named_member(sender)
                        .await
                        .unwrap_or_else(|e| {
                            warn!("获取群成员({})发生错误: {}", sender, e);
                            None
                        });

                    if let Some(m) = &member {
                        m.nickname()
                    } else {
                        warn!("群成员({})信息获取失败", sender);
                        return;
                    }
                };

                info!(
                    "{}({}) >> 群 {}({}) >> {client}: {}",
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
                client = if let Some(b) = get_bot(bot_id) {
                    b
                } else {
                    return;
                };

                let friend = if let Some(f) = client.find_friend(e.inner.from_uin) {
                    f
                } else {
                    client.refresh_friend_list().await.expect("Cannot refresh");
                    return;
                };

                info!(
                    "好友 {}({}) >> {client}: {}",
                    friend.nickname(),
                    friend.id(),
                    e.inner.elements,
                );

                let base = FriendMessageEvent::from(friend, e);

                Event::FriendMessage(base)
            }
            QEvent::DeleteFriend(e) => {
                client = get_client!(e.client);

                client.remove_friend_cache(e.inner.uin);

                Event::Unknown(EventInner::<QEvent>::from(QEvent::DeleteFriend(e)))
            }
            QEvent::GroupDisband(e) => {
                client = get_client!(e.client);

                client.remove_group_cache(e.inner.group_code);

                Event::Unknown(EventInner::<QEvent>::from(QEvent::GroupDisband(e)))
            }
            QEvent::NewMember(e) => {
                client = get_client!(e.client);
                let group_id = e.inner.group_code;
                let member = e.inner.member_uin;

                let group = if let Some(g) = client.find_group(group_id) {
                    g
                } else {
                    unwrap_result_or_print_err_return!(client.refresh_group_info(group_id).await);
                    client.find_group(group_id).unwrap()
                };

                if member == client.id() {
                } else {
                    let _member = group.get_named_member(e.inner.member_uin).await;
                }

                Event::Unknown(EventInner::<QEvent>::from(QEvent::NewMember(e)))
            }
            QEvent::GroupLeave(e) => {
                client = get_client!(e.client);
                let group_id = e.inner.group_code;
                let member = e.inner.member_uin;
                if member == client.id() {
                    client.remove_group_cache(group_id);
                } else {
                }

                Event::Unknown(EventInner::<QEvent>::from(QEvent::GroupLeave(e)))
            }
            or => Event::Unknown(EventInner::<QEvent>::from(or)),
        };

        let e = self_event.clone();
        get_listener_runtime().spawn(async move {
            get_global_listener_worker().handle(&e).await;
        });

        let _ = global_sender().send(self_event);
    }
}
