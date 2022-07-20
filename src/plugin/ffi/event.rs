use std::mem::ManuallyDrop;

use crate::Event;
use crate::plugin::Managed;

#[repr(C)]
pub struct FFIEvent {
    t: u8,
    intercepted: Managed,
    base: EventUnion,
}

impl From<Event> for FFIEvent {
    fn from(e: Event) -> Self {
        let t: u8;
        let intercepted: Managed;
        let base: EventUnion;

        fn managed_union_value<T>(value: T) -> ManuallyDrop<Managed> {
            ManuallyDrop::new(Managed::from_value(value))
        }
        match e {
            Event::BotOnlineEvent(e) => {
                t = 0;
                intercepted = e.intercept_managed();
                base = EventUnion { bot_online_event: managed_union_value(e) }
            }
            Event::GroupMessageEvent(e) => {
                t = 1;
                intercepted = e.intercept_managed();
                base = EventUnion { group_message_event: managed_union_value(e) }
            }
            Event::Unknown(e) => {
                t = 255;
                intercepted = e.intercept_managed();
                base = EventUnion { unknown: managed_union_value(e) }
            }
        }

        Self {
            t,
            intercepted,
            base,
        }
    }
}

#[repr(C)]
union EventUnion {
    group_message_event: ManuallyDrop<Managed>,
    bot_online_event: ManuallyDrop<Managed>,
    unknown: ManuallyDrop<Managed>,
}