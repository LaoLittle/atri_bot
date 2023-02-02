use super::rt::future_block_on;
use crate::client::Client;
use crate::contact::group::Group;
use crate::message;
use crate::message::forward::ForwardMessage;
use crate::message::meta::MessageReceipt;
use crate::plugin::ffi::cast_ref_phandle;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::forward::FFIForwardNode;
use atri_ffi::message::{FFIMessageChain, FFIMessageReceipt};
use atri_ffi::{Handle, ManagedCloneable, PHandle, RustStr, RustVec};
use message::MessageChain;
use std::slice;

pub unsafe fn group_to_ptr(group: Group) -> Handle {
    unsafe { std::mem::transmute(group) }
}

pub unsafe fn group_to_ptr_option(group: Option<Group>) -> Handle {
    group
        .map(|c| unsafe { group_to_ptr(c) })
        .unwrap_or_else(std::ptr::null)
}

pub extern "C" fn group_get_id(group: Handle) -> i64 {
    let group: &Group = cast_ref_phandle(&group);
    group.id()
}

pub extern "C" fn group_get_name(group: Handle) -> RustStr {
    let group: &Group = cast_ref_phandle(&group);
    let s = group.name();
    RustStr::from(s)
}

pub extern "C" fn group_get_client(group: Handle) -> PHandle {
    let group: &Group = cast_ref_phandle(&group);
    group.client() as *const Client as PHandle
}

pub extern "C" fn group_get_members(group: Handle) -> FFIFuture<RustVec<ManagedCloneable>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref_phandle(&group);

        let named: Vec<ManagedCloneable> = group
            .members()
            .await
            .into_iter()
            .map(ManagedCloneable::from_value)
            .collect();

        RustVec::from(named)
    })
}

pub extern "C" fn group_find_member(group: Handle, id: i64) -> FFIFuture<ManagedCloneable> {
    let group: &Group = cast_ref_phandle(&group);
    FFIFuture::from(async move {
        group
            .find_member(id)
            .await
            .map(ManagedCloneable::from_value)
            .unwrap_or_else(|| unsafe { ManagedCloneable::null() })
    })
}

pub extern "C" fn group_send_message(
    group: Handle,
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref_phandle(&group);
        let result = group
            .send_message(MessageChain::from_ffi(chain))
            .await
            .map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_send_message_blocking(
    manager: Handle,
    group: Handle,
    chain: FFIMessageChain,
) -> FFIResult<FFIMessageReceipt> {
    let group: &Group = cast_ref_phandle(&group);
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
    group: Handle,
    data: RustVec<u8>,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref_phandle(&group);
        let data = data.into_vec();
        let result = group
            .upload_image(data)
            .await
            .map(ManagedCloneable::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_upload_image_blocking(
    manager: Handle,
    group: Handle,
    data: RustVec<u8>,
) -> FFIResult<ManagedCloneable> {
    let group: &Group = cast_ref_phandle(&group);
    let data = data.into_vec();

    future_block_on(manager, async move {
        let result = group
            .upload_image(data)
            .await
            .map(ManagedCloneable::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_change_name(group: Handle, name: RustStr) -> FFIFuture<FFIResult<()>> {
    let s = name.as_ref().to_owned();
    FFIFuture::from(async move {
        let group: &Group = cast_ref_phandle(&group);
        let result = group.change_name(s).await;

        FFIResult::from(result)
    })
}

pub extern "C" fn group_change_name_blocking(
    manager: Handle,
    group: Handle,
    name: RustStr,
) -> FFIResult<()> {
    let s = name.as_ref().to_owned();
    let group: &Group = cast_ref_phandle(&group);

    future_block_on(manager, async move {
        let result = group.change_name(s).await;

        FFIResult::from(result)
    })
}

pub extern "C" fn group_quit(group: Handle) -> FFIFuture<bool> {
    let group: &Group = cast_ref_phandle(&group);
    FFIFuture::from(group.quit())
}

pub extern "C" fn group_quit_blocking(manager: Handle, group: Handle) -> bool {
    let group: &Group = cast_ref_phandle(&group);
    future_block_on(manager, async move { group.quit().await })
}

pub extern "C" fn group_send_forward_message(
    group: Handle,
    msg: RustVec<FFIForwardNode>,
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    let group: &Group = cast_ref_phandle(&group);
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
    manager: Handle,
    group: Handle,
    msg: RustVec<FFIForwardNode>,
) -> FFIResult<FFIMessageReceipt> {
    let group: &Group = cast_ref_phandle(&group);
    let forward = ForwardMessage::from_ffi(msg);
    future_block_on(manager, async move {
        group
            .send_forward_message(forward)
            .await
            .map(MessageReceipt::into_ffi)
            .into()
    })
}

pub extern "C" fn group_invite(group: Handle, id: i64) -> FFIFuture<FFIResult<()>> {
    let group: &Group = cast_ref_phandle(&group);
    FFIFuture::from(async move { group.invite(id).await.into() })
}

pub extern "C" fn group_invite_blocking(manager: Handle, group: Handle, id: i64) -> FFIResult<()> {
    let group: &Group = cast_ref_phandle(&group);
    future_block_on(manager, async move { group.invite(id).await.into() })
}

pub extern "C" fn group_upload_image_ex(
    group: Handle,
    ptr: *const u8,
    size: usize,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    let slice = unsafe { slice::from_raw_parts(ptr, size) };
    FFIFuture::from(async {
        let group: &Group = cast_ref_phandle(&group);
        let result = group
            .upload_image(slice)
            .await
            .map(ManagedCloneable::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn group_clone(group: Handle) -> Handle {
    let g: &Group = cast_ref_phandle(&group);
    unsafe { group_to_ptr(g.clone()) }
}

pub extern "C" fn group_drop(group: Handle) {
    drop::<Group>(unsafe { std::mem::transmute(group) })
}
