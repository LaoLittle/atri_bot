pub mod future;
pub mod event;

#[repr(C)]
pub struct Managed {
    pointer: *mut (),
    vtable: ManagedVTable,
}

#[repr(C)]
struct ManagedVTable {
    drop: extern fn(*mut ()),
}

impl Managed {
    pub fn from_value<T>(value: T) -> Self {
        let b = Box::new(value);
        let ptr = Box::into_raw(b);

        extern fn _drop<B>(pointer: *mut ()) {
            unsafe { Box::from_raw(pointer.cast::<B>()); };
        }

        Self {
            pointer: ptr.cast(),
            vtable: ManagedVTable {
                drop: _drop::<T>
            },
        }
    }

    pub fn from_static<T>(static_ref: &'static T) -> Self {
        extern fn _drop(_: *mut ()) {
            // nothing to do
        }

        Self {
            pointer: static_ref as *const _ as _,
            vtable: ManagedVTable {
                drop: _drop
            },
        }
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        (self.vtable.drop)(self.pointer);
    }
}