use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

#[repr(C)]
pub struct FFIFuture<T> {
    future_ptr: *mut (),
    fun: extern fn(*mut (), *mut ()) -> FFIPoll<T>,
}

impl<T, F> From<F> for FFIFuture<T>
    where F: Future<Output=T>
{
    fn from(fu: F) -> Self {
        let fun = poll_future::<T, F>;
        let b = Box::new(fu);
        let ptr = Box::into_raw(b);

        Self {
            future_ptr: ptr as *mut (),
            fun,
        }
    }
}

unsafe impl<T: Send> Send for FFIFuture<T> {}

unsafe impl<T: Sync> Sync for FFIFuture<T> {}

#[repr(C)]
pub struct FFIPoll<T> {
    ready: bool,
    value: MaybeUninit<T>,
}

extern fn poll_future<T, F>(f: *mut (), cx: *mut ()) -> FFIPoll<T>
    where F: Future<Output=T>
{
    let pin: Pin<&mut F> = unsafe { Pin::new_unchecked(&mut *f.cast()) };
    let cx: &mut Context = unsafe { &mut *cx.cast() };

    let poll = pin.poll(cx);
    match poll {
        Poll::Ready(value) => {
            unsafe { Box::from_raw(f.cast::<F>()); };
            FFIPoll {
                ready: true,
                value: MaybeUninit::new(value),
            }
        }
        Poll::Pending => {
            FFIPoll {
                ready: false,
                value: MaybeUninit::uninit(),
            }
        }
    }
}