use crate::contact::group::Group;
use crate::message;
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, RawVec, RustStr};

pub extern "C" fn group_get_id(group: *const ()) -> i64 {
    let group: &Group = cast_ref(group);
    group.id()
}

pub extern "C" fn group_get_name(group: *const ()) -> RustStr {
    let group: &Group = cast_ref(group);
    let s = group.name();
    RustStr::from(s)
}

pub extern "C" fn group_get_bot(group: *const ()) -> Managed {
    let group: &Group = cast_ref(group);
    Managed::from_value(group.bot().clone())
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
            .map(|receipt| Managed::from_value(receipt));

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
        let result = group
            .upload_image(data)
            .await
            .map(|img| Managed::from_value(img));

        FFIResult::from(result)
    })
}

pub extern "C" fn group_quit(group: *const ()) -> FFIFuture<bool> {
    let group: &Group = cast_ref(group);
    FFIFuture::from(group.quit())
}
