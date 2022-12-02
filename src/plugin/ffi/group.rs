use crate::contact::group::Group;
use crate::message;
use crate::plugin::cast_ref;
use crate::plugin::ffi::future_block_on;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, ManagedCloneable, RustStr, RustVec};

pub extern "C" fn group_get_id(group: *const ()) -> i64 {
    let group: &Group = cast_ref(group);
    group.id()
}

pub extern "C" fn group_get_name(group: *const ()) -> RustStr {
    let group: &Group = cast_ref(group);
    let s = group.name();
    RustStr::from(s)
}

pub extern "C" fn group_get_client(group: *const ()) -> ManagedCloneable {
    let group: &Group = cast_ref(group);
    ManagedCloneable::from_value(group.client().clone())
}

pub extern "C" fn group_get_members(group: *const ()) -> FFIFuture<RustVec<ManagedCloneable>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);

        let named: Vec<ManagedCloneable> = group
            .members()
            .await
            .into_iter()
            .map(ManagedCloneable::from_value)
            .collect();

        RustVec::from(named)
    })
}

pub extern "C" fn group_find_member(group: *const (), id: i64) -> ManagedCloneable {
    let group: &Group = cast_ref(group);
    group
        .find_member(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn group_find_or_refresh_member(
    group: *const (),
    id: i64,
) -> FFIFuture<ManagedCloneable> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        group
            .find_or_refresh_member(id)
            .await
            .map(ManagedCloneable::from_value)
            .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
    })
}

pub extern "C" fn group_send_message(
    group: *const (),
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<Managed>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let result = group
            .send_message(message::MessageChain::from_ffi(chain))
            .await
            .map(Managed::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_send_message_blocking(
    manager: *const (),
    group: *const (),
    chain: FFIMessageChain,
) -> FFIResult<Managed> {
    let group: &Group = cast_ref(group);
    let chain = message::MessageChain::from_ffi(chain);

    future_block_on(manager, async move {
        let result = group.send_message(chain).await.map(Managed::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_upload_image(
    group: *const (),
    data: RustVec<u8>,
) -> FFIFuture<FFIResult<Managed>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let data = data.into_vec();
        let result = group.upload_image(data).await.map(Managed::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_upload_image_blocking(
    manager: *const (),
    group: *const (),
    data: RustVec<u8>,
) -> FFIResult<Managed> {
    let group: &Group = cast_ref(group);
    let data = data.into_vec();

    future_block_on(manager, async move {
        let result = group.upload_image(data).await.map(Managed::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_change_name(group: *const (), name: RustStr) -> FFIFuture<FFIResult<()>> {
    let s = name.as_ref().to_owned();
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let result = group.change_name(s).await;

        FFIResult::from(result)
    })
}

pub extern "C" fn group_quit(group: *const ()) -> FFIFuture<bool> {
    let group: &Group = cast_ref(group);
    FFIFuture::from(group.quit())
}

pub extern "C" fn group_quit_blocking(manager: *const (), group: *const ()) -> bool {
    let group: &Group = cast_ref(group);
    future_block_on(manager, async move { group.quit().await })
}
