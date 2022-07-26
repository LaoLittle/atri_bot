use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ricq::handler::QEvent;
use ricq::structs::GroupMessage;
use tokio::time::error::Elapsed;

use crate::{Bot, global_receiver, MessageChain};
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

impl Event {
    pub fn intercept(&self) {
        match self {
            Self::BotOnlineEvent(e) => {
                e.intercept();
            }
            Self::GroupMessageEvent(e) => {
                e.intercept();
            }
            Self::Unknown(e) => {
                e.intercept();
            }
        }
    }

    pub fn is_intercepted(&self) -> bool {
        match self {
            Self::BotOnlineEvent(e) => {
                e.is_intercepted()
            }
            Self::GroupMessageEvent(e) => {
                e.is_intercepted()
            }
            Self::Unknown(e) => {
                e.is_intercepted()
            }
        }
    }
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

    pub fn group(&self) -> &Group {
        &self.event.group
    }

    pub fn bot(&self) -> &Bot {
        self.group().bot()
    }

    pub fn message(&self) -> &GroupMessage {
        &self.event.message
    }

    pub async fn next_event<F>(&self, timeout: Duration, filter: F) -> Result<GroupMessageEvent, Elapsed>
        where F: Fn(&GroupMessageEvent) -> bool,
    {
        tokio::time::timeout(timeout, async move {
            let mut rx = global_receiver();
            while let Ok(e) = rx.recv().await {
                if let Event::GroupMessageEvent(e) = e {
                    if self.group().id() != e.group().id() { continue; }
                    if self.message().from_uin != e.message().from_uin { continue; }

                    if !filter(&e) { continue; }
                    return e;
                }
            }

            unreachable!()
        }).await
    }

    pub async fn next_message<F>(&self, timeout: Duration, filter: F) -> Result<MessageChain, Elapsed>
        where F: Fn(&MessageChain) -> bool,
    {
        self.next_event(
            timeout,
            |e| filter(&e.message().elements),
        )
            .await
            .map(|e| e.message().elements.clone())
    }
}

impl HasSubject for GroupMessageEvent {
    fn subject(&self) -> Contact {
        Contact::Group(self.event.group.clone())
    }
}

impl FromEvent for GroupMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::GroupMessageEvent(e) = e {
            Some(e)
        } else { None }
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

pub enum MessageEvent {
    Group(GroupMessageEvent),
    Friend,
}

impl FromEvent for MessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        match e {
            Event::GroupMessageEvent(e) => Some(Self::Group(e)),
            _ => None
        }
    }
}

pub trait FromEvent: Sized {
    fn from_event(e: Event) -> Option<Self>;
}