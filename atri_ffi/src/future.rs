use crate::Managed;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

#[repr(C)]
pub struct FFIFuture<T> {
    future: Managed,
    poll: extern "C" fn(*mut (), *mut ()) -> FFIPoll<T>,
}

impl<T> FFIFuture<T> {
    pub fn from<F>(fu: F) -> Self
    where
        F: Future<Output = T>,
    {
        let fun = poll_future::<T, F>;

        Self {
            future: Managed::from_value(fu),
            poll: fun,
        }
    }

    #[inline]
    pub fn from_static<F>(fu: F) -> Self
    where
        F: Future<Output = T>,
        F: Send + 'static,
        F::Output: Send + 'static,
    {
        Self::from(fu)
    }
}

unsafe impl<T: Send> Send for FFIFuture<T> {}

unsafe impl<T: Sync> Sync for FFIFuture<T> {}

impl<T> Future for FFIFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ptr = self.future.pointer;
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

extern "C" fn poll_future<T, F>(f: *mut (), cx: *mut ()) -> FFIPoll<T>
where
    F: Future<Output = T>,
{
    let pin: Pin<&mut F> = unsafe { Pin::new_unchecked(&mut *f.cast()) };
    let cx: &mut Context = unsafe { &mut *cx.cast() };

    let poll = pin.poll(cx);
    match poll {
        Poll::Ready(value) => FFIPoll {
            ready: true,
            value: MaybeUninit::new(value),
        },
        Poll::Pending => FFIPoll {
            ready: false,
            value: MaybeUninit::uninit(),
        },
    }
}

#[repr(C)]
pub struct FFIFutureV {
    future: Managed,
    poll: extern "C" fn(*mut (), *mut ()) -> bool,
}

impl FFIFutureV {
    pub fn from<F>(fu: F) -> Self
    where
        F: Future<Output = ()>,
    {
        extern "C" fn poll_future_v<F>(f: *mut (), cx: *mut ()) -> bool
        where
            F: Future<Output = ()>,
        {
            let f = poll_future::<(), F>(f, cx);
            f.ready
        }

        let fun = poll_future_v::<F>;

        Self {
            future: Managed::from_value(fu),
            poll: fun,
        }
    }
}

unsafe impl Send for FFIFutureV {}

unsafe impl Sync for FFIFutureV {}

impl Future for FFIFutureV {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ptr = self.future.pointer;
        let fun = self.poll;

        let poll = fun(ptr, cx as *mut _ as _);

        if poll {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[repr(C)]
pub struct FFIPoll<T> {
    ready: bool,
    value: MaybeUninit<T>,
}
