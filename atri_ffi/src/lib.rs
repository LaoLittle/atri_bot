use std::mem::ManuallyDrop;
use std::{mem, slice};

pub mod closure;
pub mod error;
pub mod ffi;
pub mod future;
pub mod message;
pub mod plugin;

#[repr(C)]
pub struct Managed {
    pub pointer: *mut (),
    pub vtable: ManagedVTable,
}

unsafe impl Send for Managed {}
unsafe impl Sync for Managed {}

#[repr(C)]
pub struct ManagedVTable {
    pub drop: extern "C" fn(*mut ()),
}

impl Managed {
    pub fn from_value<T>(value: T) -> Self {
        let b = Box::new(value);
        let ptr = Box::into_raw(b);

        extern "C" fn _drop<B>(pointer: *mut ()) {
            drop(unsafe { Box::from_raw(pointer.cast::<B>()) });
        }

        Self {
            pointer: ptr.cast(),
            vtable: ManagedVTable { drop: _drop::<T> },
        }
    }

    pub fn from_static<T>(static_ref: &'static T) -> Self {
        extern "C" fn _drop(_: *mut ()) {
            // nothing to do
        }

        Self {
            pointer: static_ref as *const _ as _,
            vtable: ManagedVTable { drop: _drop },
        }
    }

    pub fn as_mut_ptr(&self) -> *mut () {
        self.pointer
    }

    pub fn as_ptr(&self) -> *const () {
        self.pointer
    }

    pub fn into_value<T>(self) -> T {
        let ptr = self.pointer;
        mem::forget(self);
        *unsafe { Box::from_raw(ptr as _) }
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        (self.vtable.drop)(self.pointer);
    }
}

pub struct ManagedRef {
    pub pointer: *mut (),
}

impl ManagedRef {
    pub fn from_ref<T>(val: &T) -> Self {
        Self {
            pointer: val as *const T as usize as *mut (),
        }
    }
}

#[repr(C)]
pub struct RustString {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl From<String> for RustString {
    fn from(s: String) -> Self {
        let mut ma = ManuallyDrop::new(s);
        let ptr = ma.as_mut_ptr();
        let len = ma.len();
        let cap = ma.capacity();

        Self {
            ptr,
            len,
            capacity: cap,
        }
    }
}

impl From<RustString> for String {
    fn from(s: RustString) -> Self {
        let str = unsafe { String::from_raw_parts(s.ptr, s.len, s.capacity) };
        str
    }
}

impl AsRef<str> for RustString {
    fn as_ref(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.ptr, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }
}

impl ToString for RustString {
    fn to_string(&self) -> String {
        self.as_ref().to_string()
    }
}

#[repr(C)]
pub struct RustStr {
    pub slice: *const u8,
    pub len: usize,
}

impl From<&str> for RustStr {
    fn from(s: &str) -> Self {
        Self {
            slice: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl AsRef<str> for RustStr {
    fn as_ref(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.slice, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }
}

impl ToString for RustStr {
    fn to_string(&self) -> String {
        self.as_ref().to_string()
    }
}

#[repr(C)]
pub struct RawVec<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

impl<T> RawVec<T> {
    pub fn into_vec(self) -> Vec<T> {
        unsafe { Vec::from_raw_parts(self.ptr, self.len, self.capacity) }
    }
}

impl<T> From<Vec<T>> for RawVec<T> {
    fn from(mut v: Vec<T>) -> Self {
        let (ptr, len, cap) = (v.as_mut_ptr(), v.len(), v.capacity());
        mem::forget(v);
        Self {
            ptr,
            len,
            capacity: cap,
        }
    }
}
