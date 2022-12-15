use crate::message::image::Image;
use crate::message::MessageChain;
use crate::plugin::cast_ref;
use atri_ffi::error::FFIResult;
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::FFIMessageChain;
use atri_ffi::{Managed, RustStr, RustString};

pub extern "C" fn message_chain_to_json(chain: FFIMessageChain) -> RustString {
    let chain = MessageChain::from_ffi(chain);
    chain.to_json().into()
}

pub extern "C" fn message_chain_from_json(json: RustStr) -> FFIResult<FFIMessageChain> {
    MessageChain::from_json(json.as_ref())
        .map(MessageChain::into_ffi)
        .into()
}

pub extern "C" fn image_get_id(img: *const ()) -> RustStr {
    let img: &Image = cast_ref(img);
    RustStr::from(img.id())
}

pub extern "C" fn _image_to_flash(img: Managed) {
    let img: Image = unsafe {
        img.into_value()
    };
    img.flash();
}

pub extern "C" fn image_get_url(img: *const ()) -> RustString {
    let img: &Image = cast_ref(img);
    RustString::from(img.url())
}
