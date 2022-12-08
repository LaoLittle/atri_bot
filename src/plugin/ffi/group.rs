use crate::contact::group::Group;
use crate::message;
use crate::message::forward::ForwardMessage;
use crate::message::meta::MessageReceipt;
use crate::plugin::cast_ref;
use crate::plugin::ffi::future_block_on;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::forward::FFIForwardNode;
use atri_ffi::message::{FFIMessageChain, FFIMessageReceipt};
use atri_ffi::{ManagedCloneable, RustStr, RustVec};
use message::MessageChain;

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
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let result = group
            .send_message(MessageChain::from_ffi(chain))
            .await
            .map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_send_message_blocking(
    manager: *const (),
    group: *const (),
    chain: FFIMessageChain,
) -> FFIResult<FFIMessageReceipt> {
    let group: &Group = cast_ref(group);
    let chain = MessageChain::from_ffi(chain);

    future_block_on(manager, async move {
        let result = group
            .send_message(chain)
            .await
            .map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_upload_image(
    group: *const (),
    data: RustVec<u8>,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let data = data.into_vec();
        let result = group
            .upload_image(data)
            .await
            .map(ManagedCloneable::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_upload_image_blocking(
    manager: *const (),
    group: *const (),
    data: RustVec<u8>,
) -> FFIResult<ManagedCloneable> {
    let group: &Group = cast_ref(group);
    let data = data.into_vec();

    future_block_on(manager, async move {
        let result = group
            .upload_image(data)
            .await
            .map(ManagedCloneable::from_value);

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

pub extern "C" fn group_change_name_blocking(
    manager: *const (),
    group: *const (),
    name: RustStr,
) -> FFIResult<()> {
    let s = name.as_ref().to_owned();
    let group: &Group = cast_ref(group);

    future_block_on(manager, async move {
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

pub extern "C" fn group_send_forward_message(
    group: *const (),
    msg: RustVec<FFIForwardNode>,
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    let group: &Group = cast_ref(group);
    let forward = ForwardMessage::from_ffi(msg);
    FFIFuture::from(async move {
        group
            .send_forward_message(forward)
            .await
            .map(MessageReceipt::into_ffi)
            .into()
    })
}

pub extern "C" fn group_send_forward_message_blocking(
    manager: *const (),
    group: *const (),
    msg: RustVec<FFIForwardNode>,
) -> FFIResult<FFIMessageReceipt> {
    let group: &Group = cast_ref(group);
    let forward = ForwardMessage::from_ffi(msg);
    future_block_on(manager, async move {
        group
            .send_forward_message(forward)
            .await
            .map(MessageReceipt::into_ffi)
            .into()
    })
}

pub extern "C" fn group_invite(group: *const (), id: i64) -> FFIFuture<FFIResult<()>> {
    let group: &Group = cast_ref(group);
    FFIFuture::from(async move { group.invite(id).await.into() })
}

pub extern "C" fn group_invite_blocking(
    manager: *const (),
    group: *const (),
    id: i64,
) -> FFIResult<()> {
    let group: &Group = cast_ref(group);
    future_block_on(manager, async move { group.invite(id).await.into() })
}
