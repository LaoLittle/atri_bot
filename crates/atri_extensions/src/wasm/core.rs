//! Explicit custom core-module ABI. This backend imports no WASI interfaces.
use crate::{Context, Extension};
use atri_core::*;
use std::{
    pin::pin,
    task::{Context as TaskContext, Poll, Waker},
    time::Duration,
};

#[link(wasm_import_module = "atri_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn import_call(ptr: u32, len: u32) -> u32;
    #[link_name = "response_len"]
    fn import_response_len(handle: u32) -> u32;
    #[link_name = "response_read"]
    fn import_response_read(handle: u32, ptr: u32, len: u32) -> u32;
    #[link_name = "response_drop"]
    fn import_response_drop(handle: u32);
}

struct Response(u32);
impl Drop for Response {
    fn drop(&mut self) {
        unsafe { import_response_drop(self.0) };
    }
}

pub(crate) async fn host_call(request: Request) -> Result<Value, ApiError> {
    let bytes = encode(&request)?;
    let handle = unsafe { import_call(bytes.as_ptr() as u32, bytes.len() as u32) };
    if handle == 0 {
        return Err(ApiError::new(
            "host_call_failed",
            "host did not allocate a response handle",
        ));
    }
    let response = Response(handle);
    let len = unsafe { import_response_len(response.0) } as usize;
    if len > MAX_CALL_BYTES {
        return Err(ApiError::new(
            "payload_too_large",
            "host response exceeds ABI limit",
        ));
    }
    let mut bytes = vec![0u8; len];
    let read = unsafe { import_response_read(response.0, bytes.as_mut_ptr() as u32, len as u32) };
    if read as usize != len {
        return Err(ApiError::new(
            "invalid_host_response",
            "host returned an incomplete response",
        ));
    }
    decode::<Result<Value, ApiError>>(&bytes)?
}

pub(crate) async fn sleep(duration: Duration) -> Result<(), ApiError> {
    let milliseconds = u64::try_from(duration.as_millis()).map_err(|_| {
        ApiError::new(
            "invalid_duration",
            "sleep duration exceeds u64 milliseconds",
        )
    })?;
    Context {}
        .invoke("tasks.sleep", &SleepRequest { milliseconds })
        .await
}

/// Allocations are always released by the guest, never the host allocator.
pub fn allocate(len: u32) -> u32 {
    if len == 0 || len as usize > MAX_CALL_BYTES {
        return 0;
    }
    Box::into_raw(vec![0u8; len as usize].into_boxed_slice()) as *mut u8 as u32
}

/// # Safety
/// `ptr,len` identifies a still-live buffer returned by `allocate` or `invoke`.
pub unsafe fn release(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(ptr as *mut u8, len as usize);
    unsafe {
        drop(Box::from_raw(slice));
    }
}

// Custom host imports complete on the host's suspended Wasmtime fiber. They
// never expose Pending to Rust. Unlike the P3 backend this ABI has no component
// task callbacks, so unrelated executor futures cannot be driven here.
fn run<F: Future<Output = Vec<u8>>>(future: F) -> Result<Vec<u8>, ApiError> {
    let mut future = pin!(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(bytes) => Ok(bytes),
        Poll::Pending => Err(ApiError::new(
            "unsupported_guest_future",
            "custom core hooks may await host calls, not external executor futures",
        )),
    }
}

fn dispatch<E: Extension + Default>(operation: u32, input: &[u8]) -> Result<Vec<u8>, ApiError> {
    match operation {
        wasm_abi::METADATA => Ok(super::describe::<E>()),
        wasm_abi::START => run(super::start::<E>()),
        wasm_abi::EVENT => run(super::on_event::<E>(input)),
        wasm_abi::SERVICE => run(super::on_service::<E>(input)),
        wasm_abi::STOP => run(super::stop()),
        _ => Err(ApiError::new(
            "unknown_operation",
            "unknown custom core operation",
        )),
    }
}

/// # Safety
/// The host provides readable `ptr,len` bytes until this invocation returns.
/// The packed output is guest-owned and must subsequently be freed by `release`.
pub unsafe fn invoke<E: Extension + Default>(operation: u32, ptr: u32, len: u32) -> u64 {
    let result = if len as usize > MAX_CALL_BYTES || (ptr == 0 && len != 0) {
        Err(ApiError::new(
            "invalid_buffer",
            "invalid custom core invocation buffer",
        ))
    } else {
        let input = if len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) }
        };
        dispatch::<E>(operation, input)
    };
    let bytes = result.unwrap_or_else(|error| super::response(Err(error)));
    let len = bytes.len() as u32;
    let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as u32;
    ((ptr as u64) << 32) | len as u64
}
