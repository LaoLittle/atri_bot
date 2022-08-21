use crate::plugin::cast_ref;
use crate::Bot;
use atri_ffi::RustStr;

pub extern "C" fn bot_get_id(bot: *const ()) -> i64 {
    let b: &Bot = cast_ref(bot);
    b.id()
}

pub extern "C" fn bot_get_nickname(bot: *const ()) -> RustStr {
    let b: &Bot = cast_ref(bot);
    RustStr::from(b.nickname())
}
