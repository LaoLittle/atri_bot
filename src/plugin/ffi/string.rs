use atri_ffi::{RustStr, RustString};
use std::ffi::{c_char, CStr, CString};
use std::ptr::null_mut;

pub extern "C" fn rust_str_cvt(str: RustStr) -> *mut c_char {
    let str = str.as_str();
    CString::new(str)
        .map(CString::into_raw)
        .unwrap_or(null_mut())
}

pub extern "C" fn c_str_cvt(ptr: *const c_char) -> RustString {
    let cstr = unsafe { CStr::from_ptr(ptr) };

    cstr.to_string_lossy().to_string().into()
}

pub extern "C" fn rust_string_drop(str: RustString) {
    drop(String::from(str));
}
