use super::cast_ref;
use super::rt::future_block_on;
use crate::contact::friend::Friend;
use crate::message::meta::MessageReceipt;
use crate::message::MessageChain;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::{FFIMessageChain, FFIMessageReceipt};
use atri_ffi::{ManagedCloneable, RustStr, RustVec};
use std::slice;

pub extern "C" fn friend_get_id(friend: *const ()) -> i64 {
    let f: &Friend = cast_ref(friend);
    f.id()
}

pub extern "C" fn friend_get_nickname(friend: *const ()) -> RustStr {
    let f: &Friend = cast_ref(friend);
    let s = f.nickname();
    RustStr::from(s)
}

pub extern "C" fn friend_get_client(friend: *const ()) -> ManagedCloneable {
    let f: &Friend = cast_ref(friend);
    ManagedCloneable::from_value(f.client().clone())
}

pub extern "C" fn friend_send_message(
    friend: *const (),
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<FFIMessageReceipt>> {
    FFIFuture::from(async move {
        let f: &Friend = cast_ref(friend);
        let chain = MessageChain::from_ffi(chain);
        let result = f.send_message(chain).await.map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn friend_send_message_blocking(
    manager: *const (),
    group: *const (),
    chain: FFIMessageChain,
) -> FFIResult<FFIMessageReceipt> {
    let group: &Friend = cast_ref(group);
    let chain = MessageChain::from_ffi(chain);

    future_block_on(manager, async move {
        let result = group
            .send_message(chain)
            .await
            .map(MessageReceipt::into_ffi);

        FFIResult::from(result)
    })
}

pub extern "C" fn friend_upload_image(
    friend: *const (),
    img: RustVec<u8>,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    FFIFuture::from(async {
        let f: &Friend = cast_ref(friend);
        let img = img.into_vec();

        let result = f.upload_image(img).await.map(ManagedCloneable::from_value);
        FFIResult::from(result)
    })
}

pub extern "C" fn friend_upload_image_blocking(
    manager: *const (),
    friend: *const (),
    data: RustVec<u8>,
) -> FFIResult<ManagedCloneable> {
    let friend: &Friend = cast_ref(friend);
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
    friend: *const (),
    ptr: *const u8,
    size: usize,
) -> FFIFuture<FFIResult<ManagedCloneable>> {
    let slice = unsafe { slice::from_raw_parts(ptr, size) };
    FFIFuture::from(async {
        let f: &Friend = cast_ref(friend);
        let result = f
            .upload_image(slice)
            .await
            .map(ManagedCloneable::from_value);
        FFIResult::from(result)
    })
}
