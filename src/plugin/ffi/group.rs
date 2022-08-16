use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use crate::contact::group::Group;
use crate::message;
use crate::plugin::cast_ref;

pub extern "C" fn group_send_message(group: *const (), chain: FFIMessageChain) -> FFIFuture<()> {
    FFIFuture::from(async {
        let group: &Group = cast_ref(group);
        let _ = group.send_message(message::MessageChain::from_ffi(chain).into()).await;
    })
}