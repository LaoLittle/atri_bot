use std::sync::Arc;
use ricq::handler::QEvent;
use crate::Bot;
use crate::contact::group::Group;

pub mod listener;

#[derive(Clone, Debug)]
pub enum Event {
    GroupMessage(GroupMessageEvent),
    BotOnline,
    Other(QEvent)
}

#[derive(Clone, Debug)]
pub struct GroupMessageEvent {
    group: Arc<Group>,
    pub(crate) inner: ricq::client::event::GroupMessageEvent,
}

impl GroupMessageEvent {

}

pub struct BotOnlineEvent {
    bot: Arc<Bot>,
}