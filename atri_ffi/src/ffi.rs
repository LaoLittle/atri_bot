use std::future::Future;
use std::marker::PhantomData;

use crate::future::FFIFuture;
use crate::Managed;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::closure::FFIFn;

#[repr(C)]
pub struct AtriVTable {
    pub plugin_manager_spawn:
    extern "C" fn(manager: *const (), FFIFuture<Managed>) -> FFIFuture<Managed>,
    pub plugin_manager_block_on:
    extern "C" fn(manager: *const (), FFIFuture<Managed>) -> Managed,
    pub new_listener:
    extern "C" fn(FFIFn<FFIFuture<bool>, FFIEvent>) -> Managed,
}

#[repr(C)]
pub struct AtriManager {
    pub manager_ptr: *const (),
    pub vtb: *const AtriVTable,
}

pub struct JoinHandle<T> {
    handle: FFIFuture<Managed>,
    _mark: PhantomData<T>,
}

impl<T> JoinHandle<T> {
    pub fn from(f: FFIFuture<Managed>) -> Self {
        Self {
            handle: f,
            _mark: PhantomData,
        }
    }
}

impl<T> Unpin for JoinHandle<T> {}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = unsafe { Pin::new_unchecked(&mut self.handle) };

        match pin.poll(cx) {
            Poll::Ready(val) => Poll::Ready(val.into_value()),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[repr(C)]
pub struct FFIEvent {
    t: u8,
    base: Managed,
}

impl FFIEvent {


    pub fn from(t: u8, e: Managed) -> Self {
        Self {
            t,
            base: e
        }
    }
}