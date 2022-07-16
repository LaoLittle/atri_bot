use std::ops::Sub;
use std::sync::Arc;

use ricq::handler::QEvent;

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
    pub(crate) inner: ricq::client::event::GroupMessageEvent,
}

impl GroupMessageEvent {}

pub struct BotOnlineEvent {
    bot: Bot,
}

pub enum Subject {
    Friend(i64),
    Group(i64)
}