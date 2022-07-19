use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ricq::handler::QEvent;
use ricq::structs::GroupMessage;

use crate::Bot;
use crate::contact::{Contact, HasSubject};
use crate::contact::group::Group;
use crate::plugin::Managed;

pub mod listener;

#[derive(Debug, Clone)]
pub enum Event {
    BotOnlineEvent(BotOnlineEvent),
    GroupMessageEvent(GroupMessageEvent),
    Unknown(EventInner<QEvent>),
}

#[derive(Debug)]
pub struct EventInner<T> {
    intercepted: Arc<AtomicBool>,
    event: Arc<T>,
}

impl<T> EventInner<T> {
    fn new(event: T) -> Self {
        Self {
            intercepted: AtomicBool::new(false).into(),
            event: event.into(),
        }
    }

    pub fn intercept(&self) {
        self.intercepted.swap(true, Ordering::Release);
    }

    pub fn is_intercepted(&self) -> bool {
        self.intercepted.load(Ordering::Relaxed)
    }

    pub(crate) fn intercept_managed(&self) -> Managed {
        Managed::from_value(self.intercepted.clone())
    }
}

impl<T> Clone for EventInner<T> {
    fn clone(&self) -> Self {
        Self {
            intercepted: self.intercepted.clone(),
            event: self.event.clone(),
        }
    }
}

pub type GroupMessageEvent = EventInner<imp::GroupMessageEvent>;

impl GroupMessageEvent {
    pub fn from(group: Group, ori: ricq::client::event::GroupMessageEvent) -> Self {
        Self::new(imp::GroupMessageEvent {
            group,
            message: ori.inner,
        })
    }

    pub fn group(&self) -> Group {
        self.event.group.clone()
    }

    pub fn message(&self) -> &GroupMessage {
        &self.event.message
    }
}

impl HasSubject for GroupMessageEvent {
    fn subject(&self) -> Contact {
        Contact::Group(self.event.group.clone())
    }
}

pub type BotOnlineEvent = EventInner<imp::BotOnlineEvent>;

impl BotOnlineEvent {
    pub fn from(bot: Bot) -> Self {
        Self::new(
            imp::BotOnlineEvent {
                bot
            }
        )
    }
}

impl EventInner<QEvent> {
    pub fn from(e: QEvent) -> Self {
        Self::new(e)
    }
}

mod imp {
    use ricq::structs::GroupMessage;

    use crate::Bot;
    use crate::contact::group::Group;

    #[derive(Debug)]
    pub struct GroupMessageEvent {
        pub group: Group,
        pub message: GroupMessage,
    }

    #[derive(Debug)]
    pub struct BotOnlineEvent {
        pub bot: Bot,
    }
}