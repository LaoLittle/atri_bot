use crate::loader::{get_plugin_handle, get_plugin_manager, get_plugin_manager_vtb};
use atri_ffi::RustString;
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::log::__log_info(0 ,format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log::__log_info(1 ,format!($($arg)*))
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::__log_info(2 ,format!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log::__log_info(3 ,format!($($arg)*))
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log::__log_info(4 ,format!($($arg)*))
    };
}

pub fn __log_info(level: u8, str: String) {
    let ffi = RustString::from(str);
    (get_plugin_manager_vtb().log)(get_plugin_handle(), get_plugin_manager(), level, ffi);
}
