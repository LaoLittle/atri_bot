//! Only C-layout data and async-ffi futures cross native module boundaries.
//! Allocations are always released by the allocating module's callback.

use crate::{API_VERSION, ApiError, MAX_CALL_BYTES, decode, encode};
use async_ffi::FfiFuture;
use serde::{Serialize, de::DeserializeOwned};

pub const ENTRY_SYMBOL: &[u8] = b"atri_extension_v1\0";
pub const FUTURE_ABI_VERSION: u32 = async_ffi::ABI_VERSION as u32;

#[repr(C)]
pub struct OwnedBuffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
    release: Option<unsafe extern "C" fn(*mut u8, usize, usize)>,
}
// ABI providers promise immutable bytes and a thread-safe origin deallocator.
unsafe impl Send for OwnedBuffer {}
unsafe impl Sync for OwnedBuffer {}
impl OwnedBuffer {
    pub fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
            release: None,
        }
    }
    pub fn from_vec(mut bytes: Vec<u8>) -> Self {
        unsafe extern "C" fn release(ptr: *mut u8, len: usize, capacity: usize) {
            if capacity != 0 {
                unsafe {
                    drop(Vec::from_raw_parts(ptr, len, capacity));
                }
            }
        }
        let result = Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            capacity: bytes.capacity(),
            release: Some(release),
        };
        std::mem::forget(bytes);
        result
    }
    /// Foreign providers are trusted native code, not a security boundary.
    pub fn as_slice(&self) -> Result<&[u8], ApiError> {
        if self.len > MAX_CALL_BYTES
            || self.len > self.capacity
            || (self.len != 0 && self.ptr.is_null())
        {
            return Err(ApiError::new("invalid_buffer", "invalid native ABI buffer"));
        }
        if self.len == 0 {
            return Ok(&[]);
        }
        Ok(unsafe { std::slice::from_raw_parts(self.ptr, self.len) })
    }
}
impl Drop for OwnedBuffer {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            unsafe {
                release(self.ptr, self.len, self.capacity);
            }
        }
    }
}

#[repr(C)]
pub struct AbiResult {
    pub status: u32,
    pub payload: OwnedBuffer,
}
impl AbiResult {
    pub fn ok<T: Serialize + ?Sized>(value: &T) -> Self {
        match encode(value) {
            Ok(bytes) => Self {
                status: 0,
                payload: OwnedBuffer::from_vec(bytes),
            },
            Err(error) => Self::error(error),
        }
    }
    pub fn error(error: ApiError) -> Self {
        match encode(&error) {
            Ok(bytes) => Self {
                status: 1,
                payload: OwnedBuffer::from_vec(bytes),
            },
            Err(_) => Self {
                status: 2,
                payload: OwnedBuffer::from_vec(b"ABI error serialization failed".to_vec()),
            },
        }
    }
    pub fn from_result<T: Serialize>(result: Result<T, ApiError>) -> Self {
        match result {
            Ok(value) => Self::ok(&value),
            Err(error) => Self::error(error),
        }
    }
    pub fn into_result<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        let bytes = self.payload.as_slice()?;
        match self.status {
            0 => decode(bytes),
            1 => Err(decode(bytes)?),
            _ => Err(ApiError::new(
                "native_error",
                String::from_utf8_lossy(bytes).into_owned(),
            )),
        }
    }
}

/// Host-owned, process-lifetime diagnostic slot. Only atomics cross the module boundary.
/// A single executor thread writes its slot; the fatal handler reads it without locks.
#[repr(C)]
pub struct ThreadActivity {
    pub sequence: std::sync::atomic::AtomicU64,
    pub operation: std::sync::atomic::AtomicU32,
    pub task_id: std::sync::atomic::AtomicU64,
}
impl Default for ThreadActivity {
    fn default() -> Self {
        Self {
            sequence: std::sync::atomic::AtomicU64::new(0),
            operation: std::sync::atomic::AtomicU32::new(0),
            task_id: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub context: usize,
    /// Enqueue work onto the host executor; returned future only waits for completion.
    pub invoke: unsafe extern "C" fn(usize, OwnedBuffer) -> FfiFuture<AbiResult>,
    /// Register the current OS thread. NULL means diagnostics are not installed.
    /// A non-null slot remains valid for the rest of the process lifetime.
    pub register_thread: unsafe extern "C" fn(usize, u64) -> *const ThreadActivity,
    /// Ordinary panic diagnostics, not a fatal-signal callback. Payload is bounded UTF-8.
    pub report_panic: unsafe extern "C" fn(usize, u64, OwnedBuffer),
}
impl HostApi {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.abi_version != API_VERSION
            || self.struct_size as usize != std::mem::size_of::<Self>()
        {
            return Err(ApiError::new(
                "host_abi_mismatch",
                "unsupported host API version or layout",
            ));
        }
        Ok(())
    }
}

#[repr(C)]
pub struct InstanceResult {
    pub instance: usize,
    pub error: OwnedBuffer,
}
pub type InstanceFuture = unsafe extern "C" fn(usize) -> FfiFuture<AbiResult>;
pub type PayloadFuture = unsafe extern "C" fn(usize, OwnedBuffer) -> FfiFuture<AbiResult>;

#[repr(C)]
pub struct NativeExtensionV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub future_abi_version: u32,
    pub metadata: unsafe extern "C" fn() -> AbiResult,
    pub create: unsafe extern "C" fn(HostApi) -> InstanceResult,
    pub start: InstanceFuture,
    pub event: PayloadFuture,
    pub service: PayloadFuture,
    pub stop: InstanceFuture,
    /// Called only after successful stop. Unresponsive instances remain resident.
    pub destroy: unsafe extern "C" fn(usize),
}
impl NativeExtensionV1 {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.abi_version != API_VERSION
            || self.struct_size as usize != std::mem::size_of::<Self>()
            || self.future_abi_version != FUTURE_ABI_VERSION
        {
            return Err(ApiError::new(
                "extension_abi_mismatch",
                "unsupported extension API, layout or future ABI",
            ));
        }
        Ok(())
    }
}
