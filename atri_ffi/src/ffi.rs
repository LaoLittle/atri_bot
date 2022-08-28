use crate::ManagedCloneable;

#[repr(C)]
pub struct AtriManager {
    pub manager_ptr: *const (),
    pub handle: usize,
    pub get_fun: extern "C" fn(sig: u16) -> *const (),
}

#[repr(C)]
pub struct FFIEvent {
    pub t: u8,
    pub intercepted: *const (),
    pub base: ManagedCloneable,
}

impl FFIEvent {
    pub fn from(t: u8, intercepted: *const (), base: ManagedCloneable) -> Self {
        Self {
            t,
            intercepted,
            base,
        }
    }

    pub fn get(self) -> (u8, *const (), ManagedCloneable) {
        (self.t, self.intercepted, self.base)
    }
}

pub trait ForFFI {
    type FFIValue;

    fn into_ffi(self) -> Self::FFIValue;

    fn from_ffi(value: Self::FFIValue) -> Self;
}
