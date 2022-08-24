use crate::plugin::cast_ref;
use crate::{get_app, Bot};
use atri_ffi::{ManagedCloneable, RawVec, RustStr};

pub extern "C" fn bot_get_id(bot: *const ()) -> i64 {
    let b: &Bot = cast_ref(bot);
    b.id()
}

pub extern "C" fn bot_get_nickname(bot: *const ()) -> RustStr {
    let b: &Bot = cast_ref(bot);
    RustStr::from(b.nickname())
}

pub extern "C" fn bot_get_list() -> RawVec<ManagedCloneable> {
    let bots: Vec<ManagedCloneable> = Bot::list()
        .into_iter()
        .map(|b| ManagedCloneable::from_value(b))
        .collect();

    RawVec::from(bots)
}

pub extern "C" fn find_bot(id: i64) -> ManagedCloneable {
    Bot::find(id)
        .map(|b| ManagedCloneable::from_value(b))
        .unwrap_or(unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_find_group(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Bot = cast_ref(bot);
    b.find_group(id)
        .map(|g| ManagedCloneable::from_value(g))
        .unwrap_or(unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_find_friend(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Bot = cast_ref(bot);
    b.find_friend(id)
        .map(|f| ManagedCloneable::from_value(f))
        .unwrap_or(unsafe { ManagedCloneable::null() })
}

pub extern "C" fn bot_get_groups(bot: *const ()) -> RawVec<ManagedCloneable> {
    let b: &Bot = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .groups()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();
    RawVec::from(ma)
}

pub extern "C" fn bot_get_friends(bot: *const ()) -> RawVec<ManagedCloneable> {
    let b: &Bot = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .friends()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();
    RawVec::from(ma)
}
