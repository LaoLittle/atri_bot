use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::null_mut;
use std::{mem, slice};

pub mod closure;
pub mod contact;
pub mod error;
pub mod ffi;
pub mod future;
pub mod message;
pub mod plugin;

#[repr(C)]
pub struct Managed {
    pub pointer: *mut (),
    pub drop: extern "C" fn(*mut ()),
}

unsafe impl Send for Managed {}
unsafe impl Sync for Managed {}

impl Managed {
    pub fn from_value<T>(value: T) -> Self {
        let b = Box::new(value);
        let ptr = Box::into_raw(b);

        extern "C" fn _drop<B>(pointer: *mut ()) {
            drop(unsafe { Box::from_raw(pointer.cast::<B>()) });
        }

        Self {
            pointer: ptr.cast(),
            drop: _drop::<T>,
        }
    }

    pub fn from_static<T>(static_ref: &'static T) -> Self {
        extern "C" fn _drop(_: *mut ()) {
            // nothing to do
        }

        Self {
            pointer: static_ref as *const _ as _,
            drop: _drop,
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

    /// for option
    pub unsafe fn null() -> Self {
        extern "C" fn _drop_null(_: *mut ()) {}

        Self {
            pointer: null_mut(),
            drop: _drop_null,
        }
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        (self.drop)(self.pointer);
    }
}

#[repr(C)]
pub struct ManagedCloneable {
    pub value: Managed,
    clone: extern "C" fn(this: *const ()) -> ManagedCloneable,
}

impl ManagedCloneable {
    pub fn from_value<T: Clone>(value: T) -> Self {
        extern "C" fn _clone<T: Clone>(this: *const ()) -> ManagedCloneable {
            let this = unsafe { &*(this as *const T) };
            ManagedCloneable::from_value(this.clone())
        }

        let value = Managed::from_value(value);
        Self {
            value,
            clone: _clone::<T>,
        }
    }

    pub fn into_value<T>(self) -> T {
        self.value.into_value()
    }

    /// for option
    pub unsafe fn null() -> Self {
        extern "C" fn _clone_null(_: *const ()) -> ManagedCloneable {
            panic!("Shouldn't call this because this is null");
        }

        Self {
            value: Managed::null(),
            clone: _clone_null,
        }
    }
}

impl Clone for ManagedCloneable {
    fn clone(&self) -> Self {
        (self.clone)(self.value.pointer)
    }
}

impl Deref for ManagedCloneable {
    type Target = Managed;

    fn deref(&self) -> &Self::Target {
        &self.value
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

impl RustStr {
    pub fn as_str<'a>(&self) -> &'a str {
        unsafe {
            let slice = slice::from_raw_parts(self.slice, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }
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
pub struct RustVec<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

unsafe impl<T: Send> Send for RustVec<T> {}
unsafe impl<T: Sync> Sync for RustVec<T> {}

impl<T> RustVec<T> {
    pub fn into_vec(self) -> Vec<T> {
        unsafe { Vec::from_raw_parts(self.ptr, self.len, self.capacity) }
    }
}

impl<T> From<Vec<T>> for RustVec<T> {
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

pub struct RustSlice<T> {
    ptr: *const T,
    len: usize,
}

impl<T> RustSlice<T> {
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T> From<&[T]> for RustSlice<T> {
    fn from(slice: &[T]) -> Self {
        Self {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

unsafe impl<T: Send> Send for RustSlice<T> {}
unsafe impl<T: Sync> Sync for RustSlice<T> {}

#[cfg(test)]
mod tests {
    use crate::{Managed, RustStr, RustString, RustVec};

    #[test]
    fn vec() {
        let v = vec![1, 1, 4, 5, 1, 4];
        let raw = RustVec::from(v);
        let v = raw.into_vec();

        assert_eq!(v, [1, 1, 4, 5, 1, 4]);
    }

    #[test]
    fn string() {
        let s = String::from("114514");
        let raw = RustString::from(s);
        let s = String::from(raw);

        assert_eq!(s, "114514");

        let slice = &s[1..];
        let raw = RustStr::from(slice);
        let slice = raw.as_ref();

        assert_eq!(slice, "14514");
    }

    #[test]
    fn managed_value() {
        #[derive(Debug, Clone)]
        struct Test {
            a: i32,
            b: usize,
            c: Option<Box<(Test, Test)>>,
        }

        impl PartialEq for Test {
            fn eq(&self, other: &Self) -> bool {
                if self.a != other.a {
                    return false;
                }
                if self.b != other.b {
                    return false;
                }

                self.c == other.c
            }
        }

        let test = Test {
            a: 233,
            b: 114514,
            c: Some(Box::new((
                Test {
                    a: 23114,
                    b: 114514,
                    c: None,
                },
                Test {
                    a: 114514,
                    b: 2333,
                    c: None,
                },
            ))),
        };
        let test0 = test.clone();
        let managed = Managed::from_value(test);
        let test: Test = managed.into_value();

        assert_eq!(test, test0);
    }
}
