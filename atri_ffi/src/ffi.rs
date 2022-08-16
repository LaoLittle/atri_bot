use std::future::Future;
use std::marker::PhantomData;

use crate::closure::FFIFn;
use crate::error::FFIResult;
use crate::future::FFIFuture;
use crate::message::FFIMessageChain;
use crate::{Managed, RawVec, RustStr};
use std::pin::Pin;
use std::task::{Context, Poll};

#[repr(C)]
pub struct AtriVTable {
    pub plugin_manager_spawn:
        extern "C" fn(manager: *const (), FFIFuture<Managed>) -> FFIFuture<Managed>,
    pub plugin_manager_block_on: extern "C" fn(manager: *const (), FFIFuture<Managed>) -> Managed,
    pub new_listener: extern "C" fn(FFIFn<FFIEvent, FFIFuture<bool>>) -> Managed,
    pub event_intercept: extern "C" fn(intercepted: *const ()),
    pub event_is_intercepted: extern "C" fn(intercepted: *const ()) -> bool,
    pub bot_get_id: extern "C" fn(bot: *const ()) -> i64,
    pub group_message_event_get_group: extern "C" fn(event: *const ()) -> Managed,
    pub group_message_event_get_message: extern "C" fn(event: *const ()) -> FFIMessageChain,
    pub group_get_id: extern "C" fn(group: *const ()) -> i64,
    pub group_get_bot: extern "C" fn(group: *const ()) -> Managed,
    pub group_send_message:
        extern "C" fn(group: *const (), chain: FFIMessageChain) -> FFIFuture<FFIResult<Managed>>,
    pub group_upload_image:
        extern "C" fn(group: *const (), data: RawVec<u8>) -> FFIFuture<FFIResult<Managed>>,

    pub log_info: extern "C" fn(log: RustStr),
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
    intercepted: *const (),
    base: Managed,
}

impl FFIEvent {
    pub fn from(t: u8, intercepted: *const (), base: Managed) -> Self {
        Self {
            t,
            intercepted,
            base,
        }
    }

    pub fn get(self) -> (u8, *const (), Managed) {
        (self.t, self.intercepted, self.base)
    }
}
