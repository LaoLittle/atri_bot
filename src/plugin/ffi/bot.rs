use crate::plugin::cast_ref;
use crate::Bot;
use atri_ffi::{ManagedCloneable, RustStr, RustVec};

pub extern "C" fn bot_get_id(bot: *const ()) -> i64 {
    let b: &Bot = cast_ref(bot);
    b.id()
}

pub extern "C" fn bot_get_nickname(bot: *const ()) -> RustStr {
    let b: &Bot = cast_ref(bot);
    RustStr::from(b.nickname())
}

pub extern "C" fn bot_get_list() -> RustVec<ManagedCloneable> {
    let bots: Vec<ManagedCloneable> = Bot::list()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(bots)
}

pub extern "C" fn find_bot(id: i64) -> ManagedCloneable {
    Bot::find(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_find_group(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Bot = cast_ref(bot);
    b.find_group(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_find_friend(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Bot = cast_ref(bot);
    b.find_friend(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_get_groups(bot: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Bot = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .groups()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}

pub extern "C" fn bot_get_friends(bot: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Bot = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .friends()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}
