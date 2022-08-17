use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ricq::handler::QEvent;
use ricq::structs::GroupMessage;

use atri_ffi::ffi::FFIEvent;
use atri_ffi::Managed;
use tokio::time::error::Elapsed;

use crate::contact::group::Group;
use crate::contact::member::{AnonymousMember, Member};
use crate::contact::{Contact, HasSubject};
use crate::{Bot, Listener, MessageChain};

pub mod listener;

#[derive(Clone)]
pub enum Event {
    BotOnlineEvent(BotOnlineEvent),
    GroupMessageEvent(GroupMessageEvent),
    FriendMessageEvent(FriendMessageEvent),
    Unknown(EventInner<QEvent>),
}

impl Event {
    pub fn into_ffi(self) -> FFIEvent {
        macro_rules! ffi_get {
            ($($e:ident => $t:expr);* $(;)?) => {
                match self {
                    $(
                    Self::$e(e) => ($t, &*e.intercepted as *const AtomicBool, Managed::from_value(e)),
                    )*
                }
            };
        }

        let (t, intercepted, base) = ffi_get! {
            BotOnlineEvent => 0;
            GroupMessageEvent => 1;
            FriendMessageEvent => 2;
            Unknown => 255;
        };

        FFIEvent::from(t, intercepted as _, base)
    }
}

macro_rules! event_impl {
    ($($variant:ident),* ;$name:ident: $ret:ty as $func:expr) => {
        impl Event {
            pub fn $name(&self) -> $ret {
                match self {
                    $(Self::$variant(e) => {
                        ($func)(e)
                    })*
                }
            }
        }
    };
}

macro_rules! event_fun_impl {
    ($($name:ident: $ret:ty as $func:expr);+ $(;)?) => {
        $(
        event_impl! {
            BotOnlineEvent,
            GroupMessageEvent,
            FriendMessageEvent,
            Unknown;
            $name: $ret as $func
        }
        )*
    };
}

event_fun_impl! {
    intercept: () as EventInner::intercept;
    is_intercepted: bool as EventInner::is_intercepted;
}

impl FromEvent for Event {
    fn from_event(e: Event) -> Option<Self> {
        Some(e)
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

    pub async fn sender(&self) -> Member {
        let id = self.message().from_uin;
        self.group()
            .find_member(id)
            .await
            .map(|named| Member::Named(named))
            .unwrap_or_else(|| {
                let an = AnonymousMember::from(self.group().clone(), id);
                Member::Anonymous(an)
            })
    }

    pub fn message(&self) -> &GroupMessage {
        &self.event.message
    }

    pub async fn next_event<F>(
        &self,
        timeout: Duration,
        filter: F,
    ) -> Result<GroupMessageEvent, Elapsed>
    where
        F: Fn(&GroupMessageEvent) -> bool,
    {
        tokio::time::timeout(timeout, async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(5);
            let group_id = self.group().id();
            let sender = self.message().from_uin;

            let guard = Listener::listening_on(move |e: GroupMessageEvent| {
                let tx = tx.clone();
                async move {
                    if group_id != e.group().id() {
                        return true;
                    }
                    if sender != e.message().from_uin {
                        return true;
                    }

                    tx.send(e).await.unwrap_or_else(|_| unreachable!());
                    false
                }
            })
            .start();

            while let Some(e) = rx.recv().await {
                if !filter(&e) {
                    continue;
                }

                drop(guard);
                return e;
            }

            unreachable!()
        })
        .await
    }

    pub async fn next_message<F>(
        &self,
        timeout: Duration,
        filter: F,
    ) -> Result<MessageChain, Elapsed>
    where
        F: Fn(&MessageChain) -> bool,
    {
        self.next_event(timeout, |e| filter(&e.message().elements))
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
        } else {
            None
        }
    }
}

pub type FriendMessageEvent = EventInner<imp::FriendMessageEvent>;

impl FriendMessageEvent {}

impl FromEvent for FriendMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::FriendMessageEvent(e) = e {
            Some(e)
        } else {
            None
        }
    }
}

pub type BotOnlineEvent = EventInner<imp::BotOnlineEvent>;

impl BotOnlineEvent {
    pub fn from(bot: Bot) -> Self {
        Self::new(imp::BotOnlineEvent { bot })
    }
}

impl EventInner<QEvent> {
    pub fn from(e: QEvent) -> Self {
        Self::new(e)
    }
}

mod imp {
    use ricq::structs::{FriendMessage, GroupMessage};

    use crate::contact::group::Group;
    use crate::Bot;

    pub struct GroupMessageEvent {
        pub group: Group,
        pub message: GroupMessage,
    }

    pub struct FriendMessageEvent {
        pub message: FriendMessage,
    }

    pub struct BotOnlineEvent {
        pub bot: Bot,
    }
}

pub enum MessageEvent {
    Group(GroupMessageEvent),
    Friend(FriendMessageEvent),
}

impl FromEvent for MessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        match e {
            Event::GroupMessageEvent(e) => Some(Self::Group(e)),
            Event::FriendMessageEvent(e) => Some(Self::Friend(e)),
            _ => None,
        }
    }
}

pub trait FromEvent: Sized {
    fn from_event(e: Event) -> Option<Self>;
}
