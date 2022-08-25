use crate::contact::group::Group;
use crate::message;
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, ManagedCloneable, RawVec, RustStr, RustString};

pub extern "C" fn group_get_id(group: *const ()) -> i64 {
    let group: &Group = cast_ref(group);
    group.id()
}

pub extern "C" fn group_get_name(group: *const ()) -> RustStr {
    let group: &Group = cast_ref(group);
    let s = group.name();
    RustStr::from(s)
}

pub extern "C" fn group_get_bot(group: *const ()) -> ManagedCloneable {
    let group: &Group = cast_ref(group);
    ManagedCloneable::from_value(group.bot().clone())
}

pub extern "C" fn group_get_members(group: *const ()) -> FFIFuture<RawVec<ManagedCloneable>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);

        let named: Vec<ManagedCloneable> = group
            .members()
            .await
            .into_iter()
            .map(ManagedCloneable::from_value)
            .collect();

        RawVec::from(named)
    })
}

pub extern "C" fn group_find_member(group: *const (), id: i64) -> ManagedCloneable {
    let group: &Group = cast_ref(group);
    group
        .find_member(id)
        .map(ManagedCloneable::from_value)
        .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
}

pub extern "C" fn group_get_named_member(group: *const (), id: i64) -> FFIFuture<ManagedCloneable> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        group
            .get_named_member(id)
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

pub extern "C" fn group_upload_image(
    group: *const (),
    data: RawVec<u8>,
) -> FFIFuture<FFIResult<Managed>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let data = data.into_vec();
        let result = group.upload_image(data).await.map(Managed::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_change_name(
    group: *const (),
    name: RustString,
) -> FFIFuture<FFIResult<()>> {
    let s = String::from(name);
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
