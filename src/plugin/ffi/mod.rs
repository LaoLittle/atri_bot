pub mod client;
pub mod env;
pub mod event;
pub mod friend;
pub mod group;
pub mod listener;
pub mod log;
pub mod member;
pub mod message;
pub mod rt;

fn cast_ref<'a, T>(ptr: *const ()) -> &'a T {
    unsafe { &*(ptr as *const T) }
}

fn cast_ref_t<'a, T>(r: &*const ()) -> &'a T {
    unsafe {
        let ptr = r as *const *const () as *const T;
        &*(ptr)
    }
}

fn _cast_ref_mut<'a, T>(ptr: *mut ()) -> &'a mut T {
    unsafe { &mut *(ptr as *mut T) }
}
