use std::sync::Arc;
use atri_ffi::ffi::FFIEvent;
use atri_ffi::Managed;
use crate::bot::Bot;
use crate::contact::group::Group;
use crate::loader::get_plugin_manager_vtb;

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
}

#[derive(Clone)]
pub struct FriendMessageEvent(EventInner);

macro_rules! event_impl {
    ($($e:ty);* $(;)?) => {
        $(
        impl $e {
            pub fn intercept(&self) {
                self.0.intercept();
            }

            pub fn is_intercepted(&self) -> bool {
                self.0.is_intercepted()
            }
        }
        )*
    };
}

event_impl! {
    BotOnlineEvent;
    GroupMessageEvent;
    FriendMessageEvent;
}

/*pub trait FromFFIEvent: Sized {
    fn from_ffi(e: FFIEvent) -> Option<Self>;
}

macro_rules! from_ffi_impl {
    ($($e:ty => $t:expr);* $(;)?) => {
        $(
        impl FromFFIEvent for $e {
            fn from_ffi(e: FFIEvent) -> Option<Self> {
                match e.get() {
                    ($t,m) => Some(Self(Arc::new(m))),
                    _ => None
                }
            }
        }
        )*
    };
}

from_ffi_impl! {
    BotOnlineEvent => 0;
    GroupMessageEvent => 1;
    FriendMessageEvent => 2;
}*/