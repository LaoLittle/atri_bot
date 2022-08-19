use crate::error::{AtriError, AtriResult};
use atri_ffi::error::FFIResult;
use atri_ffi::future::FFIFuture;
use atri_ffi::Managed;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub mod manager;

pub struct JoinHandle<T> {
    handle: FFIFuture<FFIResult<Managed>>,
    _mark: PhantomData<T>,
}

impl<T> JoinHandle<T> {
    pub fn from(f: FFIFuture<FFIResult<Managed>>) -> Self {
        Self {
            handle: f,
            _mark: PhantomData,
        }
    }
}

impl<T> Unpin for JoinHandle<T> {}

impl<T> Future for JoinHandle<T> {
    type Output = AtriResult<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = unsafe { Pin::new_unchecked(&mut self.handle) };

        match pin.poll(cx) {
            Poll::Ready(ffi) => {
                let result = match Result::from(ffi) {
                    Ok(val) => Ok(val.into_value()),
                    Err(s) => Err(AtriError::JoinError(s)),
                };
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
