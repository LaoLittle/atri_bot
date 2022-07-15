use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

#[repr(C)]
pub struct FFIFuture<T> {
    future_ptr: *mut (),
    fun: extern fn(*mut (), *mut ()) -> FFIPoll<T>,
}

unsafe impl<T: Send> Send for FFIFuture<T> {}

unsafe impl<T: Sync> Sync for FFIFuture<T> {}

#[repr(C)]
pub struct FFIPoll<T> {
    ready: bool,
    value: MaybeUninit<T>,
}

impl<T> Future for FFIFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ptr = self.future_ptr;
        let fun = self.fun;

        let poll = fun(ptr, cx as *mut _ as _);

        if poll.ready {
            let val = unsafe { poll.value.assume_init() };
            Poll::Ready(val)
        } else {
            Poll::Pending
        }
    }
}