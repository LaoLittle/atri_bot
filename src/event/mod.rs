


use ricq::handler::QEvent;
use ricq::structs::GroupMessage;

use crate::Bot;
use crate::contact::group::Group;

pub mod listener;

#[derive(Clone, Debug)]
pub enum Event {
    GroupMessage(GroupMessageEvent),
    BotOnline,
    Other(QEvent),
}

#[derive(Clone, Debug)]
pub struct GroupMessageEvent {
    group: Group,
    message: GroupMessage,
}

impl GroupMessageEvent {
    pub fn from(group: Group, event: ricq::client::event::GroupMessageEvent) -> Self {
        
        Self {
            group,
            message: event.inner
        }
    }
}

pub struct BotOnlineEvent {
    bot: Bot,
}

pub enum Subject {
    Friend(i64),
    Group(i64)
}