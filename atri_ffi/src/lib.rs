use std::mem;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;

pub mod error;
pub mod ffi;
pub mod future;
pub mod plugin;

#[repr(C)]
pub struct Managed {
    pointer: *mut (),
    vtable: ManagedVTable,
}

unsafe impl Send for Managed {}
unsafe impl Sync for Managed {}

#[repr(C)]
struct ManagedVTable {
    drop: extern "C" fn(*mut ()),
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

#[repr(C)]
pub struct RawString {
    pointer: *mut u8,
    length: usize,
    capacity: usize,
}

impl RawString {
    pub fn null() -> Self {
        Self {
            pointer: null_mut(),
            length: 0,
            capacity: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.pointer.is_null()
    }

    pub fn to_string(self) -> Option<String> {
        if self.is_null() {
            return None;
        }

        Some(unsafe { String::from_raw_parts(self.pointer, self.length, self.capacity) })
    }
}

impl From<String> for RawString {
    fn from(s: String) -> Self {
        let mut ma = ManuallyDrop::new(s);
        let ptr = ma.as_mut_ptr();
        let len = ma.len();
        let cap = ma.capacity();

        Self {
            pointer: ptr,
            length: len,
            capacity: cap,
        }
    }
}
