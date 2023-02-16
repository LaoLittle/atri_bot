use super::rt::future_block_on;
use crate::contact::friend::Friend;
use crate::message::meta::MessageReceipt;
use crate::message::MessageChain;
use crate::plugin::ffi::cast_ref_phandle;
use crate::plugin::ffi::client::client_to_handle;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::{FFIMessageChain, FFIMessageReceipt};
use atri_ffi::{Handle, ManagedCloneable, RustStr, RustVec};
use std::slice;

pub unsafe fn friend_to_ptr(friend: Friend) -> Handle {
    unsafe { std::mem::transmute(friend) }
}

pub unsafe fn friend_to_ptr_option(friend: Option<Friend>) -> Handle {
    friend
        .map(|c| unsafe { friend_to_ptr(c) })
        .unwrap_or_else(std::ptr::null)
}

pub extern "C" fn friend_get_id(friend: Handle) -> i64 {
    let f: &Friend = cast_ref_phandle(&friend);
    f.id()
}

pub extern "C" fn friend_get_nickname(friend: Handle) -> RustStr {
    let f: &Friend = cast_ref_phandle(&friend);
    let s = f.nickname();
    RustStr::from(s)
}

pub extern "C" fn friend_get_client(friend: Handle) -> Handle {
    let f: &Friend = cast_ref_phandle(&friend);
    unsafe { client_to_handle(f.client()) }
}

pub extern "C" fn friend_send_message(
    friend: Handle,
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    FFIFuture::from(async move {
        let f: &Friend = cast_ref_phandle(&friend);
        let chain = MessageChain::from_ffi(chain);
        let result = f.send_message(chain).await.map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn friend_send_message_blocking(
    manager: Handle,
    friend: Handle,
    chain: FFIMessageChain,
) -> FFIResult<FFIMessageReceipt> {
    let friend: &Friend = cast_ref_phandle(&friend);
    let chain = MessageChain::from_ffi(chain);

    future_block_on(manager, async move {
        let result = friend
            .send_message(chain)
            .await
            .map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn friend_upload_image(
    friend: Handle,
    img: RustVec<u8>,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    FFIFuture::from(async {
        let f: &Friend = cast_ref_phandle(&friend);
        let img = img.into_vec();

        let result = f.upload_image(img).await.map(ManagedCloneable::from_value);
        FFIResult::from(result)
    })
}

pub extern "C" fn friend_upload_image_blocking(
    manager: Handle,
    friend: Handle,
    data: RustVec<u8>,
) -> FFIResult<ManagedCloneable> {
    let friend: &Friend = cast_ref_phandle(&friend);
    let data = data.into_vec();

    future_block_on(manager, async move {
        let result = friend
            .upload_image(data)
            .await
            .map(ManagedCloneable::from_value);

        FFIResult::from(result)
    })
}

pub extern "C" fn friend_upload_image_ex(
    friend: Handle,
    ptr: *const u8,
    size: usize,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    let slice = unsafe { slice::from_raw_parts(ptr, size) };
    FFIFuture::from(async {
        let f: &Friend = cast_ref_phandle(&friend);
        let result = f
            .upload_image(slice)
            .await
            .map(ManagedCloneable::from_value);
        FFIResult::from(result)
    })
}

pub extern "C" fn friend_clone(friend: Handle) -> Handle {
    let f: &Friend = cast_ref_phandle(&friend);
    unsafe { friend_to_ptr(f.clone()) }
}

pub extern "C" fn friend_drop(friend: Handle) {
    drop::<Friend>(unsafe { std::mem::transmute(friend) })
}
