//! Custom core-module backend. It has no WASI imports or ambient capabilities.
use super::{DEADLINE, FUEL, MEMORY, decode_response, execution_error};
use crate::{native::HostBridge, runtime::Inner};
use atri_core::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use wasmtime::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

const HANDLES: usize = 64;
struct State {
    runtime: Weak<Inner>,
    id: String,
    cancel: CancellationToken,
    responses: BTreeMap<u32, Vec<u8>>,
    next: u32,
    limits: StoreLimits,
}
struct Guest {
    store: Store<State>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    invoke: TypedFunc<(u32, u32, u32), u64>,
}
pub(crate) struct CoreModule {
    engine: Engine,
    module: Module,
    id: String,
}
pub(crate) struct CoreInstance {
    // A callback owns the entire store until it completes. Trap, timeout, or
    // cancellation drops that store instead of re-entering abandoned guest code.
    guest: Mutex<Option<Guest>>,
    failed: AtomicBool,
}
struct HostJob(tokio::task::JoinHandle<Result<Value, ApiError>>);
impl Drop for HostJob {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn error(message: impl Into<String>) -> wasmtime::Error {
    wasmtime::Error::msg(message.into())
}
fn memory(caller: &mut Caller<'_, State>) -> wasmtime::Result<Memory> {
    caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| error("guest does not export memory"))
}
fn bytes(caller: &mut Caller<'_, State>, ptr: u32, len: u32) -> wasmtime::Result<Vec<u8>> {
    if len as usize > MAX_CALL_BYTES {
        return Err(error("guest call exceeds payload limit"));
    }
    let memory = memory(caller)?;
    let end = (ptr as usize)
        .checked_add(len as usize)
        .ok_or_else(|| error("guest range overflow"))?;
    Ok(memory
        .data(&*caller)
        .get(ptr as usize..end)
        .ok_or_else(|| error("guest memory range out of bounds"))?
        .to_vec())
}
fn linker(engine: &Engine) -> Result<Linker<State>, ApiError> {
    let mut linker = Linker::new(engine);
    linker
        .func_wrap_async(
            wasm_abi::IMPORT_MODULE,
            "call",
            |mut caller: Caller<'_, State>, (ptr, len): (u32, u32)| {
                Box::new(async move {
                    let request: Request =
                        decode(&bytes(&mut caller, ptr, len)?).map_err(|e| error(e.to_string()))?;
                    let state = caller.data();
                    let runtime = state.runtime.upgrade();
                    let id = state.id.clone();
                    let cancel = state.cancel.clone();
                    let result = if let Some(runtime) = runtime {
                        let mut job = HostJob(tokio::spawn(async move {
                            runtime.host(&id, cancel, request).await
                        }));
                        (&mut job.0).await.map_err(|e| error(e.to_string()))?
                    } else {
                        Err(ApiError::new(
                            "metadata_host_call",
                            "host calls are unavailable while discovering metadata",
                        ))
                    };
                    let response = encode(&result).map_err(|e| error(e.to_string()))?;
                    let state = caller.data_mut();
                    let total: usize = state.responses.values().map(Vec::len).sum();
                    if state.responses.len() >= HANDLES
                        || total
                            .checked_add(response.len())
                            .is_none_or(|n| n > MAX_CALL_BYTES)
                    {
                        return Err(error("guest response handle quota exceeded"));
                    }
                    state.next = state.next.wrapping_add(1).max(1);
                    while state.responses.contains_key(&state.next) {
                        state.next = state.next.wrapping_add(1).max(1);
                    }
                    let id = state.next;
                    state.responses.insert(id, response);
                    Ok(id)
                })
            },
        )
        .map_err(ApiError::internal)?;
    linker
        .func_wrap(
            wasm_abi::IMPORT_MODULE,
            "response_len",
            |caller: Caller<'_, State>, handle: u32| -> wasmtime::Result<u32> {
                caller
                    .data()
                    .responses
                    .get(&handle)
                    .map(|b| b.len() as u32)
                    .ok_or_else(|| error("unknown guest response handle"))
            },
        )
        .map_err(ApiError::internal)?;
    linker
        .func_wrap(
            wasm_abi::IMPORT_MODULE,
            "response_read",
            |mut caller: Caller<'_, State>,
             handle: u32,
             ptr: u32,
             len: u32|
             -> wasmtime::Result<u32> {
                let response = caller
                    .data_mut()
                    .responses
                    .remove(&handle)
                    .ok_or_else(|| error("unknown guest response handle"))?;
                if len as usize != response.len() {
                    caller.data_mut().responses.insert(handle, response);
                    return Err(error("guest response size mismatch"));
                }
                let memory = memory(&mut caller)?;
                let result = memory
                    .write(&mut caller, ptr as usize, &response)
                    .map_err(|e| error(e.to_string()));
                let length = response.len() as u32;
                caller.data_mut().responses.insert(handle, response);
                result?;
                Ok(length)
            },
        )
        .map_err(ApiError::internal)?;
    linker
        .func_wrap(
            wasm_abi::IMPORT_MODULE,
            "response_drop",
            |mut caller: Caller<'_, State>, handle: u32| {
                caller.data_mut().responses.remove(&handle);
            },
        )
        .map_err(ApiError::internal)?;
    Ok(linker)
}
impl CoreModule {
    pub async fn load(source: Vec<u8>) -> Result<(Arc<Self>, ExtensionMetadata), ApiError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|error| ApiError::new("wasm_engine", error.to_string()))?;
        let compiler = engine.clone();
        let compiled = tokio::task::spawn_blocking(move || Module::new(&compiler, source))
            .await
            .map_err(|error| {
                ApiError::new(
                    "wasm_compile",
                    format!("core compiler task failed: {error}"),
                )
            })?
            .map_err(|error| {
                ApiError::new(
                    "wasm_compile",
                    format!("core compilation failed: {error:#}"),
                )
            })?;
        let mut module = Self {
            engine,
            module: compiled,
            id: String::new(),
        };
        let guest = module.instantiate().await?;
        let guest = CoreInstance {
            guest: Mutex::new(Some(guest)),
            failed: AtomicBool::new(false),
        };
        let metadata = guest.invoke(wasm_abi::METADATA, Vec::new()).await?;
        crate::runtime::check_notice(&metadata)?;
        let metadata: ExtensionMetadata = from_value(&metadata)?;
        crate::runtime::validate_metadata(&metadata)?;
        module.id = metadata.id.clone();
        Ok((Arc::new(module), metadata))
    }
    async fn instantiate(&self) -> Result<Guest, ApiError> {
        let state = State {
            runtime: Weak::new(),
            id: String::new(),
            cancel: CancellationToken::new(),
            responses: BTreeMap::new(),
            next: 0,
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY)
                .memories(1)
                .tables(4)
                .table_elements(100_000)
                .instances(1)
                .build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(FUEL).map_err(ApiError::internal)?;
        store
            .fuel_async_yield_interval(Some(100_000))
            .map_err(ApiError::internal)?;
        let instance = tokio::time::timeout(
            Duration::from_secs(10),
            linker(&self.engine)?.instantiate_async(&mut store, &self.module),
        )
        .await
        .map_err(|_| ApiError::new("wasm_timeout", "guest instantiation exceeded deadline"))?
        .map_err(execution_error)?;
        let version = instance
            .get_typed_func::<(), u32>(&mut store, "atri_abi_version")
            .map_err(ApiError::internal)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| ApiError::new("wasm_exports", "missing memory"))?;
        let version =
            tokio::time::timeout(Duration::from_secs(5), version.call_async(&mut store, ()))
                .await
                .map_err(|_| ApiError::new("wasm_timeout", "guest ABI query exceeded deadline"))?
                .map_err(execution_error)?;
        if version != API_VERSION {
            return Err(ApiError::new(
                "extension_abi_mismatch",
                "unsupported core-module ABI version",
            ));
        }
        let alloc = instance
            .get_typed_func(&mut store, "atri_alloc")
            .map_err(ApiError::internal)?;
        let free = instance
            .get_typed_func(&mut store, "atri_free")
            .map_err(ApiError::internal)?;
        let invoke = instance
            .get_typed_func(&mut store, "atri_invoke")
            .map_err(ApiError::internal)?;
        Ok(Guest {
            store,
            memory,
            alloc,
            free,
            invoke,
        })
    }
    pub async fn create(&self, bridge: HostBridge) -> Result<Arc<CoreInstance>, ApiError> {
        if bridge.id != self.id {
            return Err(ApiError::new(
                "extension_id_mismatch",
                "host caller differs from discovered metadata",
            ));
        }
        let mut guest = self.instantiate().await?;
        let state = guest.store.data_mut();
        state.runtime = bridge.runtime;
        state.id = bridge.id;
        state.cancel = bridge.cancel;
        Ok(Arc::new(CoreInstance {
            guest: Mutex::new(Some(guest)),
            failed: AtomicBool::new(false),
        }))
    }
}
impl CoreInstance {
    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
            && self.guest.try_lock().is_ok_and(|guest| guest.is_none())
    }
    pub async fn invoke(&self, operation: u32, payload: Vec<u8>) -> Result<Value, ApiError> {
        if payload.len() > MAX_CALL_BYTES {
            return Err(ApiError::new(
                "payload_too_large",
                "guest input exceeds limit",
            ));
        }
        let mut slot = self.guest.lock().await;
        let Some(mut guest) = slot.take() else {
            return if operation == wasm_abi::STOP {
                Ok(Value::Null)
            } else {
                Err(ApiError::new(
                    "wasm_failed",
                    "core module was discarded after failed execution",
                ))
            };
        };
        self.failed.store(true, Ordering::Release);
        let Guest {
            store,
            memory,
            alloc,
            free,
            invoke,
        } = &mut guest;
        store.set_fuel(FUEL).map_err(ApiError::internal)?;
        let cancel = store.data().cancel.clone();
        let invocation = async {
            let ptr = alloc
                .call_async(&mut *store, payload.len() as u32)
                .await
                .map_err(execution_error)?;
            memory
                .write(&mut *store, ptr as usize, &payload)
                .map_err(ApiError::internal)?;
            let output = invoke
                .call_async(&mut *store, (operation, ptr, payload.len() as u32))
                .await
                .map_err(execution_error)?;
            free.call_async(&mut *store, (ptr, payload.len() as u32))
                .await
                .map_err(execution_error)?;
            let (ptr, len) = ((output >> 32) as u32, output as u32);
            if len as usize > MAX_CALL_BYTES {
                return Err(ApiError::new(
                    "payload_too_large",
                    "guest output exceeds limit",
                ));
            }
            let mut bytes = vec![0; len as usize];
            memory
                .read(&*store, ptr as usize, &mut bytes)
                .map_err(ApiError::internal)?;
            free.call_async(&mut *store, (ptr, len))
                .await
                .map_err(execution_error)?;
            Ok(bytes)
        };
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled(), if operation != wasm_abi::STOP => return Err(ApiError::cancelled()),
            result = tokio::time::timeout(DEADLINE, invocation) => result,
        };
        let bytes = result
            .map_err(|_| ApiError::new("wasm_timeout", "guest invocation exceeded deadline"))??;
        self.failed.store(false, Ordering::Release);
        *slot = Some(guest);
        decode_response(&bytes)
    }
}
