use crate::loader::get_plugin_manager_vtb;
use atri_ffi::RustStr;
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::__log_info(format!($($arg)*))
    };
}

pub fn __log_info<S: AsRef<str>>(str: S) {
    let s = str.as_ref();
    let ffi = RustStr::from(s);
    (get_plugin_manager_vtb().log_info)(ffi);
}
