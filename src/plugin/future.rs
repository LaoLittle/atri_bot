use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

#[repr(C)]
pub struct FFIFuture<T> {
    future_ptr: *mut (),
    poll: extern fn(*mut (), *mut ()) -> FFIPoll<T>,
    drop: extern fn(*mut ()),
}

impl<T> FFIFuture<T> {
    pub fn from<F>(fu: F) -> Self
        where F: Future<Output=T>
    {
        extern fn _drop<T>(ptr: *mut ()) {
            drop(unsafe { Box::from_raw(ptr as *mut T) });
        }

        let fun = poll_future::<T, F>;
        let b = Box::new(fu);
        let ptr = Box::into_raw(b);

        Self {
            future_ptr: ptr as *mut (),
            poll: fun,
            drop: _drop::<F>,
        }
    }
}

unsafe impl<T: Send> Send for FFIFuture<T> {}

unsafe impl<T: Sync> Sync for FFIFuture<T> {}

impl<T> Drop for FFIFuture<T> {
    fn drop(&mut self) {
        (self.drop)(self.future_ptr)
    }
}

#[repr(C)]
pub struct FFIPoll<T> {
    ready: bool,
    value: MaybeUninit<T>,
}

impl<T> Future for FFIFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ptr = self.future_ptr;
        let fun = self.poll;

        let poll = fun(ptr, cx as *mut _ as _);

        if poll.ready {
            let val = unsafe { poll.value.assume_init() };
            Poll::Ready(val)
        } else {
            Poll::Pending
        }
    }
}

extern fn poll_future<T, F>(f: *mut (), cx: *mut ()) -> FFIPoll<T>
    where F: Future<Output=T>
{
    let pin: Pin<&mut F> = unsafe { Pin::new_unchecked(&mut *f.cast()) };
    let cx: &mut Context = unsafe { &mut *cx.cast() };

    let poll = pin.poll(cx);
    match poll {
        Poll::Ready(value) => {
            drop(unsafe { Box::from_raw(f.cast::<F>()) });
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