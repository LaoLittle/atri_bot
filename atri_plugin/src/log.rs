use crate::loader::{get_plugin_handle, get_plugin_manager, get_plugin_manager_vtb};
use atri_ffi::{RustString};
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::__log_info(format!($($arg)*))
    };
}

pub fn __log_info(str: String) {
    let ffi = RustString::from(str);
    (get_plugin_manager_vtb().log_info)(get_plugin_handle(),get_plugin_manager(),ffi);
}
