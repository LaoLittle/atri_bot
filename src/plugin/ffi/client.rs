use super::cast_ref;
use crate::Client;
use atri_ffi::{ManagedCloneable, RustString, RustVec};

pub extern "C" fn find_client(id: i64) -> ManagedCloneable {
    Client::find(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_get_id(client: *const ()) -> i64 {
    let b: &Client = cast_ref(client);
    b.id()
}

pub extern "C" fn client_get_nickname(client: *const ()) -> RustString {
    let b: &Client = cast_ref(client);
    RustString::from(b.nickname())
}

pub extern "C" fn client_get_list() -> RustVec<ManagedCloneable> {
    let clients: Vec<ManagedCloneable> = Client::list()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(clients)
}

pub extern "C" fn client_find_group(client: *const (), id: i64) -> ManagedCloneable {
    let b: &Client = cast_ref(client);
    b.find_group(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_find_friend(client: *const (), id: i64) -> ManagedCloneable {
    let b: &Client = cast_ref(client);
    b.find_friend(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn client_get_groups(client: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Client = cast_ref(client);
    let ma: Vec<ManagedCloneable> = b
        .groups()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}

pub extern "C" fn client_get_friends(client: *const ()) -> RustVec<ManagedCloneable> {
    let b: &Client = cast_ref(client);
    let ma: Vec<ManagedCloneable> = b
        .friends()
        .into_iter()
        .map(ManagedCloneable::from_value)
        .collect();

    RustVec::from(ma)
}
