use crate::runtime::Inner;
use async_ffi::{FfiFuture, FutureExt};
use atri_core::{abi::*, *};
use futures::channel::oneshot;
use libloading::Library;
use std::{
    path::Path,
    sync::{
        Arc, LazyLock, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{runtime::Handle, sync::Notify};
use tokio_util::sync::CancellationToken;

/// Native code and every address supplied to it remain mapped until process exit.
/// Logical removal is deliberately not dlclose.
pub(crate) struct NativeModule {
    api: NativeExtensionV1,
    _library: Library,
}
static MODULES: LazyLock<Mutex<Vec<Arc<NativeModule>>>> = LazyLock::new(Mutex::default);
static BRIDGES: LazyLock<Mutex<Vec<Arc<HostBridge>>>> = LazyLock::new(Mutex::default);
pub(crate) struct HostBridge {
    pub runtime: Weak<Inner>,
    pub id: String,
    pub cancel: CancellationToken,
    pub handle: Handle,
}
pub(crate) struct NativeInstance {
    module: Arc<NativeModule>,
    pointer: usize,
    active: AtomicUsize,
    idle: Notify,
    destroyed: AtomicBool,
    admission: Mutex<()>,
}
struct Active<'a>(&'a NativeInstance);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}
unsafe fn read_table(ptr: *const NativeExtensionV1) -> Result<NativeExtensionV1, ApiError> {
    if ptr.is_null() {
        return Err(ApiError::new("extension_abi_mismatch", "null native entry"));
    }
    let header = ptr.cast::<u32>();
    let (version, size) = unsafe { (header.read(), header.add(1).read()) };
    if version != API_VERSION || size as usize != std::mem::size_of::<NativeExtensionV1>() {
        return Err(ApiError::new(
            "extension_abi_mismatch",
            "unsupported native API or table size",
        ));
    }
    if unsafe { header.add(2).read() } != FUTURE_ABI_VERSION {
        return Err(ApiError::new(
            "extension_abi_mismatch",
            "unsupported future ABI",
        ));
    }
    // Only the validated full-size table can be interpreted as callback fields.
    let api = unsafe { ptr.read() };
    api.validate()?;
    Ok(api)
}
impl NativeModule {
    pub unsafe fn load(path: &Path) -> Result<(Arc<Self>, ExtensionMetadata), ApiError> {
        // Native extensions are trusted machine code. Validate the fixed header before
        // reading any function pointer or calling extension metadata.
        let library = unsafe { Library::new(path) }.map_err(ApiError::internal)?;
        let entry = unsafe {
            library.get::<unsafe extern "C" fn() -> *const NativeExtensionV1>(ENTRY_SYMBOL)
        }
        .map_err(ApiError::internal)?;
        let ptr = unsafe { entry() };
        let api = unsafe { read_table(ptr) }?;
        let metadata = unsafe { (api.metadata)() }.into_result()?;
        let module = Arc::new(Self {
            api,
            _library: library,
        });
        MODULES.lock().unwrap().push(module.clone());
        Ok((module, metadata))
    }
    pub fn create(self: &Arc<Self>, bridge: HostBridge) -> Result<Arc<NativeInstance>, ApiError> {
        let bridge = Arc::new(bridge);
        let host = HostApi {
            abi_version: API_VERSION,
            struct_size: std::mem::size_of::<HostApi>() as u32,
            context: Arc::as_ptr(&bridge) as usize,
            invoke: host_invoke,
            register_thread: host_register_thread,
            report_panic: host_report_panic,
        };
        BRIDGES.lock().unwrap().push(bridge);
        let result = unsafe { (self.api.create)(host) };
        if result.instance == 0 {
            return Err(decode(result.error.as_slice()?)
                .unwrap_or_else(|_| ApiError::new("native_create", "extension creation failed")));
        }
        Ok(Arc::new(NativeInstance {
            module: self.clone(),
            pointer: result.instance,
            active: AtomicUsize::new(0),
            idle: Notify::new(),
            destroyed: AtomicBool::new(false),
            admission: Mutex::new(()),
        }))
    }
}
impl NativeInstance {
    pub async fn invoke(&self, operation: u32, payload: Vec<u8>) -> Result<Value, ApiError> {
        {
            let _admission = self.admission.lock().unwrap();
            if self.destroyed.load(Ordering::Acquire) {
                return Err(ApiError::cancelled());
            }
            self.active.fetch_add(1, Ordering::AcqRel);
        }
        let _active = Active(self);
        let api = &self.module.api;
        let result = unsafe {
            match operation {
                wasm_abi::START => (api.start)(self.pointer),
                wasm_abi::STOP => (api.stop)(self.pointer),
                wasm_abi::EVENT => (api.event)(self.pointer, OwnedBuffer::from_vec(payload)),
                wasm_abi::SERVICE => (api.service)(self.pointer, OwnedBuffer::from_vec(payload)),
                _ => return Err(ApiError::unsupported("native operation")),
            }
        }
        .await;
        result.into_result()
    }
    pub async fn destroy_after_stop(&self) {
        // Close admission before waiting: callbacks which acquired an instance Arc
        // before logical disable can never enter after destruction.
        {
            let _admission = self.admission.lock().unwrap();
            if self.destroyed.swap(true, Ordering::AcqRel) {
                return;
            }
        }
        loop {
            let idle = self.idle.notified();
            if self.active.load(Ordering::Acquire) == 0 {
                break;
            }
            idle.await;
        }
        unsafe { (self.module.api.destroy)(self.pointer) };
    }
}
// No Drop destroys an instance: a timed out worker may still use its pointer.
unsafe extern "C" fn host_register_thread(context: usize, thread_id: u64) -> *const ThreadActivity {
    if context == 0 {
        return std::ptr::null();
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bridge = unsafe { &*(context as *const HostBridge) };
        crate::crash::register_thread(&bridge.id, thread_id)
    }))
    .unwrap_or(std::ptr::null())
}

unsafe extern "C" fn host_report_panic(context: usize, thread_id: u64, input: OwnedBuffer) {
    if context == 0 {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bridge = unsafe { &*(context as *const HostBridge) };
        if let Ok(bytes) = input.as_slice()
            && let Ok(report) = std::str::from_utf8(bytes)
        {
            crate::crash::record_panic(&bridge.id, thread_id, report);
        }
    }));
}

unsafe extern "C" fn host_invoke(context: usize, input: OwnedBuffer) -> FfiFuture<AbiResult> {
    let bridge = unsafe { &*(context as *const HostBridge) };
    let request = input.as_slice().and_then(decode::<Request>);
    let runtime = bridge.runtime.clone();
    let id = bridge.id.clone();
    let cancel = bridge.cancel.clone();
    let (mut sender, receiver) = oneshot::channel();
    bridge.handle.spawn(async move {
        let operation = async {
            match request {
                Err(error) => Err(error),
                Ok(request) => match runtime.upgrade() {
                    Some(inner) => inner.host(&id, cancel, request).await,
                    None => Err(ApiError::cancelled()),
                },
            }
        };
        tokio::select! {
            result = operation => { let _ = sender.send(AbiResult::from_result(result)); },
            _ = sender.cancellation() => {},
        }
    });
    // This future never polls Tokio code across a dynamic-library TLS boundary.
    async move {
        receiver
            .await
            .unwrap_or_else(|_| AbiResult::error(ApiError::cancelled()))
    }
    .into_ffi()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_small_legacy_prefix_without_reading_callbacks() {
        let legacy = [API_VERSION, 8u32];
        assert!(unsafe { read_table(legacy.as_ptr().cast()) }.is_err());
        let future = [
            API_VERSION,
            std::mem::size_of::<NativeExtensionV1>() as u32,
            FUTURE_ABI_VERSION.wrapping_add(1),
        ];
        assert!(unsafe { read_table(future.as_ptr().cast()) }.is_err());
        assert!(unsafe { read_table(std::ptr::null()) }.is_err());
    }
}
