use std::ops::Deref;
use std::sync::Arc;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::Managed;
use crate::bot::Bot;
use crate::contact::group::Group;
use crate::loader::get_plugin_manager_vtb;
use crate::message::MessageChain;

pub enum Event {
    BotOnlineEvent(BotOnlineEvent),
    GroupMessageEvent(GroupMessageEvent),
    FriendMessageEvent(FriendMessageEvent),
    Unknown(EventInner),
}

impl Event {
    pub fn from_ffi(ffi: FFIEvent) -> Self {
        let (t,intercepted,m) = ffi.get();
        let arc = Arc::new(m);
        let inner = EventInner {
            intercepted,
            event: arc
        };

        match t {
            0 => Self::BotOnlineEvent(BotOnlineEvent(inner)),
            1 => Self::GroupMessageEvent(GroupMessageEvent(inner)),
            2 => Self::FriendMessageEvent(FriendMessageEvent(inner)),
            _ => Self::Unknown(inner)
        }
    }
}

impl FromEvent for Event {
    fn from_event(e: Event) -> Option<Self> {
        Some(e)
    }
}

#[derive(Clone)]
pub struct EventInner {
    intercepted: *const (),
    event: Arc<Managed>,
}

impl EventInner {
    pub fn intercept(&self) {
        (get_plugin_manager_vtb().event_intercept)(self.intercepted)
    }

    pub fn is_intercepted(&self) -> bool {
        (get_plugin_manager_vtb().event_is_intercepted)(self.intercepted)
    }
}

unsafe impl Send for EventInner {}

unsafe impl Sync for EventInner {}

#[derive(Clone)]
pub struct BotOnlineEvent(EventInner);

#[derive(Clone)]
pub struct GroupMessageEvent(EventInner);

impl GroupMessageEvent {
    pub fn bot(&self) -> Bot {
        let ma = (get_plugin_manager_vtb().group_message_event_get_bot)(self.0.event.pointer);
        Bot(ma)
    }

    pub fn group(&self) -> Group {
        let ma = (get_plugin_manager_vtb().group_message_event_get_group)(self.0.event.pointer);
        Group(ma)
    }

    pub fn message(&self) -> MessageChain {
        let ffi = (get_plugin_manager_vtb().group_message_event_get_message)(self.0.event.pointer);
        MessageChain::from_ffi(ffi)
    }
}

impl FromEvent for GroupMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::GroupMessageEvent(e) = e {
            Some(e)
        } else { None }
    }
}

#[derive(Clone)]
pub struct FriendMessageEvent(EventInner);

impl FromEvent for FriendMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::FriendMessageEvent(e) = e {
            Some(e)
        } else { None }
    }
}

pub trait FromEvent: Sized {
    fn from_event(e: Event) -> Option<Self>;
}

macro_rules! event_inner_impl {
    ($($t:ty)*) => {
        $(
        impl Deref for $t {
            type Target = EventInner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        )*
    };
}

event_inner_impl! {
    BotOnlineEvent
    GroupMessageEvent
    FriendMessageEvent
}