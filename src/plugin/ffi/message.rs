use crate::message::image::Image;
use crate::plugin::cast_ref;
use atri_ffi::{Managed, RustStr, RustString};

pub extern "C" fn image_get_id(img: *const ()) -> RustStr {
    let img: &Image = cast_ref(img);
    RustStr::from(img.id())
}

pub extern "C" fn image_to_flash(img: Managed) {
    let img: Image = img.into_value();
    img.flash();
}

pub extern "C" fn image_get_url(img: *const ()) -> RustString {
    let img: &Image = cast_ref(img);
    RustString::from(img.url())
}
