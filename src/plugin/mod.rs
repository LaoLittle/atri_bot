pub mod ffi;

fn cast_ref<'a, T>(ptr: *const ()) -> &'a T {
    unsafe { &*(ptr as *const T) }
}

fn cast_ref_mut<'a, T>(ptr: *mut ()) -> &'a mut T {
    unsafe { &mut *(ptr as *mut T) }
}
