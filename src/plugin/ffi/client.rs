use crate::plugin::ffi::cast_ref_phandle;
use crate::plugin::ffi::friend::{friend_to_ptr, friend_to_ptr_option};
use crate::plugin::ffi::group::{group_to_handle, group_to_ptr_option};
use crate::Client;
use atri_ffi::{Handle, RustString, RustVec};

#[inline]
pub unsafe fn client_to_handle(client: Client) -> Handle {
    unsafe { std::mem::transmute(client) }
}

pub unsafe fn client_to_handle_option(client: Option<Client>) -> Handle {
    client
        .map(|c| unsafe { client_to_handle(c) })
        .unwrap_or_else(std::ptr::null)
}

pub extern "C" fn find_client(id: i64) -> Handle {
    unsafe { client_to_handle_option(Client::find(id)) }
}

pub extern "C" fn client_get_id(client: Handle) -> i64 {
    let b: &Client = cast_ref_phandle(&client);
    b.id()
}

pub extern "C" fn client_get_nickname(client: Handle) -> RustString {
    let b: &Client = cast_ref_phandle(&client);
    RustString::from(b.nickname())
}

pub extern "C" fn client_get_list() -> RustVec<Handle> {
    let clients: Vec<Handle> = Client::list()
        .into_iter()
        .map(|c| unsafe { client_to_handle(c) })
        .collect();

    RustVec::from(clients)
}

pub extern "C" fn client_find_group(client: Handle, id: i64) -> Handle {
    let b: &Client = cast_ref_phandle(&client);

    unsafe { group_to_ptr_option(b.find_group(id)) }
}

pub extern "C" fn client_find_friend(client: Handle, id: i64) -> Handle {
    let b: &Client = cast_ref_phandle(&client);

    unsafe { friend_to_ptr_option(b.find_friend(id)) }
}

pub extern "C" fn client_get_groups(client: Handle) -> RustVec<Handle> {
    let b: &Client = cast_ref_phandle(&client);
    let ma: Vec<Handle> = b
        .groups()
        .into_iter()
        .map(|g| unsafe { group_to_handle(g) })
        .collect();

    RustVec::from(ma)
}

pub extern "C" fn client_get_friends(client: Handle) -> RustVec<Handle> {
    let b: &Client = cast_ref_phandle(&client);
    let ma: Vec<Handle> = b
        .friends()
        .into_iter()
        .map(|f| unsafe { friend_to_ptr(f) })
        .collect();

    RustVec::from(ma)
}

pub extern "C" fn client_clone(client: Handle) -> Handle {
    let b: &Client = cast_ref_phandle(&client);
    unsafe { client_to_handle(b.clone()) }
}

pub extern "C" fn client_drop(client: Handle) {
    drop::<Client>(unsafe { std::mem::transmute(client) })
}
