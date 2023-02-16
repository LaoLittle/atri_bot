use std::fmt::Debug;
use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tracing::{error, info, warn};

use crate::contact::friend::Friend;
use crate::contact::member::{AnonymousMember, Member, NamedMember};
use crate::event::{
    ClientLoginEvent, DeleteFriendEvent, Event, FriendMessageEvent, FriendPokeEvent,
    GroupMessageEvent, GroupPokeEvent, NewFriendEvent,
};
use crate::global_listener_worker;
use crate::{global_listener_runtime, global_status, Client};

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
        let client: Client;

        fn get_client(id: i64) -> Option<Client> {
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
                client = if let Some(b) = get_client(id) {
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
                        "{client} >> 群[{}({})]: {}",
                        group_name(),
                        group_id,
                        message(),
                    );
                    return;
                }

                let Some(group) = client.find_or_refresh_group(group_id).await else {
                    cannot_find_group(group_id);
                    error_more_info(&e);

                    return;
                };

                let sender = e.inner.from_uin;

                let member: Member;
                let holder: NamedMember;
                let nick = if sender == AnonymousMember::ID {
                    let Some(info) = e.inner.elements.anonymous() else {
                        error!("获取匿名信息失败. Raw event: {:?}", e.inner);
                        return;
                    };

                    member = Member::Anonymous(AnonymousMember::from(&group, info.into()));
                    "匿名"
                } else {
                    match group.try_refresh_member(sender).await {
                        Ok(Some(m)) => {
                            holder = m.clone();
                            member = Member::Named(m);
                            holder.nickname()
                        }
                        Ok(None) => {
                            warn!(
                                "群成员({})信息获取失败, 或许成员已不在本群. RawEvent: {:?}",
                                sender, e.inner
                            );
                            return;
                        }
                        Err(err) => {
                            error!(
                                "获取群成员({})发生错误: {}. RawEvent: {:?}",
                                sender, err, e.inner
                            );
                            return;
                        }
                    }
                };

                info!(
                    "{}({}) >> 群[{}({})] >> {client}: {}",
                    nick,
                    sender,
                    group_name(),
                    group_id,
                    message(),
                );

                let base = GroupMessageEvent::from(group, member, e);
                Event::GroupMessage(base)
            }
            QEvent::FriendMessage(e) => {
                client = get_client!(e.client);

                let friend_id = e.inner.from_uin;

                let Some(friend) = client.find_or_refresh_friend_list(friend_id).await else {
                    error!("无法找到好友: {}({})", e.inner.from_nick, friend_id);
                    return;
                };

                info!("{friend} >> {client}: {}", e.inner.elements,);

                let base = FriendMessageEvent::from(friend, e);

                Event::FriendMessage(base)
            }
            QEvent::NewFriend(e) => {
                client = get_client!(e.client);

                let f = Friend::from(&client, e.inner);
                client.cache_friend(f.clone());

                info!("{f}已添加");

                Event::NewFriend(NewFriendEvent::from(f))
            }
            QEvent::DeleteFriend(e) => {
                client = get_client!(e.client);

                let id = e.inner.uin;
                let Some(f) = client.remove_friend_cache(id) else {
                    return;
                };

                info!("{f}已删除");

                Event::DeleteFriend(DeleteFriendEvent::from(f))
            }
            QEvent::FriendPoke(e) => {
                client = get_client!(e.client);
                let friend_id = e.inner.sender;

                let Some(f) = client.find_friend(friend_id) else {
                    error!("寻找好友{friend_id}失败");
                    return;
                };

                info!("{f}戳了戳{client}");

                Event::FriendPoke(FriendPokeEvent::from(f))
            }
            QEvent::GroupPoke(e) => {
                client = get_client!(e.client);
                let group_id = e.inner.group_code;
                let Some(group) = client.find_or_refresh_group(group_id).await else {
                    cannot_find_group(group_id);
                    error_more_info(&e);

                    return;
                };

                let sender = e.inner.sender;
                let Some(sender) = group.find_member(sender).await else {
                    error!("无法找到群成员{sender}, Raw event: {:?}", e);
                    return;
                };

                let target = e.inner.sender;
                let Some(target) = group.find_member(target).await else {
                    error!("无法找到群成员{target}, Raw event: {:?}", e);
                    return;
                };

                info!("{sender}戳了戳{target} >> {group} >> {client}");

                Event::GroupPoke(GroupPokeEvent::from(group, sender, target))
            }
            QEvent::GroupDisband(e) => {
                client = get_client!(e.client);

                let group_id = e.inner.group_code;
                let op_id = e.inner.operator_uin;

                if let Some(g) = client.find_or_refresh_group(group_id).await {
                    let member = g.members_cache().get(&op_id).and_then(|r| r.to_owned());

                    let name = member
                        .map(|n| n.card_name().to_owned())
                        .unwrap_or_else(|| op_id.to_string());
                    info!("群#{}({})解散, 操作人: {}", g.name(), g.id(), name);
                } else {
                    info!("群({})解散, 操作人: {}", group_id, op_id);
                }

                client.remove_group_cache(e.inner.group_code);

                Event::Unknown(QEvent::GroupDisband(e).into())
            }
            QEvent::NewMember(e) => {
                client = get_client!(e.client);
                let group_id = e.inner.group_code;
                let member_id = e.inner.member_uin;

                let Some(group) = client.find_or_refresh_group(group_id).await else {
                    cannot_find_group(group_id);
                    error_more_info(&e);

                    return;
                };

                if member_id != client.id() {
                    let _member = group.try_refresh_member(member_id).await;
                }

                Event::Unknown(QEvent::NewMember(e).into())
            }
            QEvent::GroupLeave(e) => {
                client = get_client!(e.client);
                let group_id = e.inner.group_code;
                let member = e.inner.member_uin;
                if member == client.id() {
                    client.remove_group_cache(group_id);
                } else {
                    let Some(group) = client.find_group(group_id) else {
                        return; // already removed?
                    };

                    group.remove_member_cache(member);
                }

                Event::Unknown(QEvent::GroupLeave(e).into())
            }
            QEvent::KickedOffline(e) => {
                client = get_client!(e.client);

                info!("{}下线, Kicked: {:?}", client, e);

                global_status().remove_client(client.id());

                Event::Unknown(QEvent::KickedOffline(e).into())
            }
            QEvent::MSFOffline(e) => {
                client = get_client!(e.client);

                info!("{}下线, MSF: {:?}", client, e);

                global_status().remove_client(client.id());

                Event::Unknown(QEvent::MSFOffline(e).into())
            }
            or => {
                info!("Other event: {:?}", or);

                Event::Unknown(or.into())
            }
        };

        global_listener_runtime().spawn(async move {
            global_listener_worker().handle(&self_event).await;

            let _ = global_sender().send(self_event);
        });
    }
}

fn cannot_find_group(group_id: i64) {
    error!("无法找到群({}), 这是一个Bug, 请报告此问题", group_id);
}

fn error_more_info<D: Debug>(d: &D) {
    error!("额外信息: {:?}", d);
}
