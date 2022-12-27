use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use atri_ffi::ffi::FFIEvent;
use atri_ffi::ManagedCloneable;
use ricq::handler::QEvent;

use crate::contact::friend::Friend;
use crate::contact::group::Group;
use crate::contact::member::{Member, NamedMember};
use crate::contact::{Contact, ContactSubject};
use crate::message::MessageChain;
use crate::{Client, Listener};

pub mod custom;
pub mod listener;

#[derive(Clone)]
pub enum Event {
    ClientLogin(ClientLoginEvent),
    GroupMessage(GroupMessageEvent),
    FriendMessage(FriendMessageEvent),
    NewFriend(NewFriendEvent),
    DeleteFriend(DeleteFriendEvent),
    FriendPoke(FriendPokeEvent),
    GroupPoke(GroupPokeEvent),
    Unknown(SharedEvent<QEvent>),
}

impl Event {
    pub fn into_ffi(self) -> FFIEvent {
        macro_rules! ffi_get {
            ($($e:ident => $t:expr);* $(;)?) => {
                match self {
                    $(
                    Self::$e(e) => ($t, &e.event.intercepted as *const AtomicBool, ManagedCloneable::from_value(e)),
                    )*
                }
            };
        }

        let (t, intercepted, base) = ffi_get! {
            ClientLogin => 0;
            GroupMessage => 1;
            FriendMessage => 2;
            NewFriend => 3;
            DeleteFriend => 4;
            FriendPoke => 5;
            GroupPoke => 6;
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
            ClientLogin,
            GroupMessage,
            FriendMessage,
            NewFriend,
            DeleteFriend,
            FriendPoke,
            GroupPoke,
            Unknown;
            $name: $ret as $func
        }
        )*
    };
}

event_fun_impl! {
    intercept: () as SharedEvent::intercept;
    is_intercepted: bool as SharedEvent::is_intercepted;
}

impl FromEvent for Event {
    fn from_event(e: Event) -> Option<Self> {
        Some(e)
    }
}

#[derive(Debug)]
pub struct SharedEvent<T> {
    event: Arc<EventWithFlag<T>>,
}

impl<T> SharedEvent<T> {
    pub fn inner(&self) -> &T {
        &self.event.inner
    }
}

impl<T> Clone for SharedEvent<T> {
    fn clone(&self) -> Self {
        Self {
            event: self.event.clone(),
        }
    }
}

#[derive(Debug)]
struct EventWithFlag<T> {
    intercepted: AtomicBool,
    inner: T,
}

impl<T> SharedEvent<T> {
    fn new(event: T) -> Self {
        Self {
            event: EventWithFlag {
                intercepted: AtomicBool::new(false),
                inner: event,
            }
            .into(),
        }
    }

    pub fn intercept(&self) {
        self.event.intercepted.store(true, Ordering::Relaxed);
    }

    pub fn is_intercepted(&self) -> bool {
        self.event.intercepted.load(Ordering::Relaxed)
    }

    pub fn try_into_inner(self) -> Result<T, Self> {
        let e = Arc::try_unwrap(self.event);

        match e {
            Ok(e) => Ok(e.inner),
            Err(arc) => Err(Self { event: arc }),
        }
    }
}

pub type GroupMessageEvent = SharedEvent<imp::GroupMessageEvent>;

impl GroupMessageEvent {
    pub fn group(&self) -> &Group {
        &self.inner().group
    }

    pub fn client(&self) -> &Client {
        self.group().client()
    }

    pub fn sender(&self) -> Member {
        self.inner().sender.clone()
    }

    pub fn message(&self) -> &MessageChain {
        &self.inner().message
    }

    pub async fn next_event<F>(&self, timeout: Duration, filter: F) -> Option<GroupMessageEvent>
    where
        F: Fn(&GroupMessageEvent) -> bool,
    {
        let group_id = self.group().id();
        let sender_id = self.sender().id();

        Listener::next_event(timeout, |e: &GroupMessageEvent| {
            if e.group().id() != group_id {
                return false;
            }

            if e.message().metadata().sender != sender_id {
                return false;
            }

            filter(e)
        })
        .await
    }

    pub async fn next_message<F>(&self, timeout: Duration, filter: F) -> Option<MessageChain>
    where
        F: Fn(&MessageChain) -> bool,
    {
        self.next_event(timeout, |e| filter(e.message()))
            .await
            .map(|e: GroupMessageEvent| match e.try_into_inner() {
                Ok(e) => e.message,
                Err(e) => e.message().clone(),
            })
    }

    pub(crate) fn from(
        group: Group,
        sender: Member,
        ori: ricq::client::event::GroupMessageEvent,
    ) -> Self {
        Self::new(imp::GroupMessageEvent {
            group,
            sender,
            message: ori.inner.into(),
        })
    }
}

impl ContactSubject for GroupMessageEvent {
    fn subject(&self) -> Contact {
        Contact::Group(self.group().clone())
    }
}

impl FromEvent for GroupMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::GroupMessage(e) = e {
            Some(e)
        } else {
            None
        }
    }
}

pub type FriendMessageEvent = SharedEvent<imp::FriendMessageEvent>;

impl FriendMessageEvent {
    pub fn friend(&self) -> &Friend {
        &self.inner().friend
    }

    pub fn client(&self) -> &Client {
        self.friend().client()
    }

    pub fn message(&self) -> &MessageChain {
        &self.inner().message
    }

    pub(crate) fn from(friend: Friend, ori: ricq::client::event::FriendMessageEvent) -> Self {
        let imp = imp::FriendMessageEvent {
            friend,
            message: ori.inner.into(),
        };

        Self::new(imp)
    }
}

impl FromEvent for FriendMessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        if let Event::FriendMessage(e) = e {
            Some(e)
        } else {
            None
        }
    }
}

impl ContactSubject for FriendMessageEvent {
    fn subject(&self) -> Contact {
        Contact::Friend(self.friend().clone())
    }
}

pub type ClientLoginEvent = SharedEvent<imp::ClientLoginEvent>;

impl ClientLoginEvent {
    pub fn client(&self) -> &Client {
        &self.inner().client
    }
}

impl ClientLoginEvent {
    pub(crate) fn from(bot: Client) -> Self {
        Self::new(imp::ClientLoginEvent { client: bot })
    }
}

pub type NewFriendEvent = SharedEvent<imp::NewFriendEvent>;

impl NewFriendEvent {
    pub fn friend(&self) -> &Friend {
        &self.inner().friend
    }

    pub fn client(&self) -> &Client {
        self.friend().client()
    }
}

impl NewFriendEvent {
    pub(crate) fn from(friend: Friend) -> Self {
        Self::new(imp::NewFriendEvent { friend })
    }
}

pub type DeleteFriendEvent = SharedEvent<imp::DeleteFriendEvent>;

impl DeleteFriendEvent {
    pub fn friend(&self) -> &Friend {
        &self.inner().friend
    }

    pub fn client(&self) -> &Client {
        self.friend().client()
    }
}

impl DeleteFriendEvent {
    pub(crate) fn from(friend: Friend) -> Self {
        Self::new(imp::DeleteFriendEvent { friend })
    }
}

pub type FriendPokeEvent = SharedEvent<imp::FriendPokeEvent>;

impl FriendPokeEvent {
    pub fn friend(&self) -> &Friend {
        &self.inner().friend
    }

    pub fn client(&self) -> &Client {
        self.friend().client()
    }
}

impl FriendPokeEvent {
    pub(crate) fn from(friend: Friend) -> Self {
        Self::new(imp::FriendPokeEvent { friend })
    }
}

pub type GroupPokeEvent = SharedEvent<imp::GroupPokeEvent>;

impl GroupPokeEvent {
    pub fn group(&self) -> &Group {
        &self.inner().group
    }

    pub fn client(&self) -> &Client {
        self.group().client()
    }

    pub fn sender(&self) -> &NamedMember {
        &self.inner().sender
    }

    pub fn target(&self) -> &NamedMember {
        &self.inner().target
    }
}

impl GroupPokeEvent {
    pub(crate) fn from(group: Group, sender: NamedMember, target: NamedMember) -> Self {
        Self::new(imp::GroupPokeEvent {
            group,
            sender,
            target,
        })
    }
}

impl From<QEvent> for SharedEvent<QEvent> {
    fn from(value: QEvent) -> Self {
        Self::new(value)
    }
}

mod imp {

    use crate::contact::friend::Friend;
    use crate::contact::group::Group;
    use crate::contact::member::{Member, NamedMember};
    use crate::message::MessageChain;
    use crate::Client;

    pub struct ClientLoginEvent {
        pub client: Client,
    }

    pub struct GroupMessageEvent {
        pub group: Group,
        pub sender: Member,
        pub message: MessageChain,
    }

    pub struct FriendMessageEvent {
        pub friend: Friend,
        pub message: MessageChain,
    }

    pub struct NewFriendEvent {
        pub friend: Friend,
    }

    pub struct DeleteFriendEvent {
        pub friend: Friend, // for information purpose
    }

    pub struct FriendPokeEvent {
        pub friend: Friend,
    }

    pub struct GroupPokeEvent {
        pub group: Group,
        pub sender: NamedMember,
        pub target: NamedMember,
    }
}

pub enum MessageEvent {
    Group(GroupMessageEvent),
    Friend(FriendMessageEvent),
}

impl FromEvent for MessageEvent {
    fn from_event(e: Event) -> Option<Self> {
        match e {
            Event::GroupMessage(e) => Some(Self::Group(e)),
            Event::FriendMessage(e) => Some(Self::Friend(e)),
            _ => None,
        }
    }
}

pub trait FromEvent: Sized {
    fn from_event(e: Event) -> Option<Self>;
}
