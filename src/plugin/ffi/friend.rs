use crate::contact::friend::Friend;
use crate::message::MessageChain;
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::future::FFIFuture;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, RustStr};

pub extern "C" fn friend_get_id(friend: *const ()) -> i64 {
    let f: &Friend = cast_ref(friend);
    f.id()
}

pub extern "C" fn friend_get_nickname(friend: *const ()) -> RustStr {
    let f: &Friend = cast_ref(friend);
    let s = f.nickname();
    RustStr::from(s)
}

pub extern "C" fn friend_get_bot(friend: *const ()) -> Managed {
    let f: &Friend = cast_ref(friend);
    Managed::from_value(f.bot().clone())
}

pub extern "C" fn friend_send_message(
    friend: *const (),
    chain: FFIMessageChain,
) -> FFIFuture<FFIResult<Managed>> {
    FFIFuture::from(async move {
        let f: &Friend = cast_ref(friend);
        let chain = MessageChain::from_ffi(chain);
        let result = f
            .send_message(chain)
            .await
            .map(|receipt| Managed::from_value(receipt));

        FFIResult::from(result)
    })
}
