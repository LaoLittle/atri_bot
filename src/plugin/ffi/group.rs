use crate::contact::group::Group;
use crate::message;
use crate::message::{Image, MessageValue};
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::{FFIMessageChain, FFIMessageValue};
use atri_ffi::{Managed, RawVec};

pub extern "C" fn group_get_id(group: *const ()) -> i64 {
    let group: &Group = cast_ref(group);
    group.id()
}

pub extern "C" fn group_send_message(
    group: *const (),
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<Managed>> {
    FFIFuture::from(async move {
        let group: &Group = cast_ref(group);
        let result = group
            .send_message(message::MessageChain::from_ffi(chain).into())
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
            .map(|img| Image::Group(img))
            .map(|img| Managed::from_value(img));

        FFIResult::from(result)
    })
}
