use crate::plugin::cast_ref;
use crate::Client;
use atri_ffi::{ManagedCloneable, RustString, RustVec};

pub extern "C" fn find_client(id: i64) -> ManagedCloneable {
    Client::find(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_get_id(bot: *const ()) -> i64 {
    let b: &Client = cast_ref(bot);
    b.id()
}

pub extern "C" fn client_get_nickname(bot: *const ()) -> RustString {
    let b: &Client = cast_ref(bot);
    RustString::from(b.nickname())
}

pub extern "C" fn client_get_list() -> RustVec<ManagedCloneable> {
    let bots: Vec<ManagedCloneable> = Client::list()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(bots)
}

pub extern "C" fn client_find_group(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Client = cast_ref(bot);
    b.find_group(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_find_friend(bot: *const (), id: i64) -> ManagedCloneable {
    let b: &Client = cast_ref(bot);
    b.find_friend(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_get_groups(bot: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Client = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .groups()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}

pub extern "C" fn client_get_friends(bot: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Client = cast_ref(bot);
    let ma: Vec<ManagedCloneable> = b
        .friends()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}
