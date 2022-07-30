use std::mem::ManuallyDrop;
use crate::Managed;

#[repr(C)]
pub struct AtriVTable {

}

#[repr(C)]
pub struct FFIEvent {
    t: u8,
    base: EventUnion,
}

#[repr(C)]
union EventUnion {
    group_message_event: ManuallyDrop<Managed>,
    bot_online_event: ManuallyDrop<Managed>,
    unknown: ManuallyDrop<Managed>,
}