use crate::{native::HostBridge, runtime::Inner};
use atri_core::*;
use std::{
    path::Path,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use wasmtime::component::{Accessor, Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::clocks::{WasiClocks, WasiClocksCtx, WasiClocksCtxView, WasiClocksView};

#[path = "core_wasm.rs"]
mod core;
#[path = "wasm_preflight.rs"]
mod preflight;

fn load_error(path: &Path, stage: &str, error: ApiError) -> ApiError {
    ApiError::new(
        error.code,
        format!(
            "WebAssembly extension {} ({stage}): {}",
            path.display(),
            error.message
        ),
    )
}

pub(crate) enum WasmModule {
    Component(Arc<ComponentModule>),
    Core(Arc<core::CoreModule>),
}
pub(crate) enum WasmInstance {
    Component(Arc<ComponentInstance>),
    Core(Arc<core::CoreInstance>),
}
impl WasmModule {
    pub async fn load(path: &Path) -> Result<(Arc<Self>, ExtensionMetadata), ApiError> {
        let source = tokio::fs::read(path).await.map_err(|error| {
            load_error(path, "read", ApiError::new("wasm_io", error.to_string()))
        })?;
        let source = match wat::parse_bytes(&source).map_err(|error| {
            load_error(
                path,
                "decode",
                ApiError::new("wasm_format", error.to_string()),
            )
        })? {
            std::borrow::Cow::Borrowed(_) => source,
            std::borrow::Cow::Owned(bytes) => bytes,
        };
        // Reject incompatible interfaces while this is still only a binary
        // inspection: no engine, compiler job, guest start, or metadata call.
        match preflight::inspect(&source).map_err(|error| load_error(path, "preflight", error))? {
            preflight::Backend::Component => {
                let (module, metadata) = ComponentModule::load(source)
                    .await
                    .map_err(|error| load_error(path, "component load/metadata", error))?;
                Ok((Arc::new(Self::Component(module)), metadata))
            }
            preflight::Backend::Core => {
                let (module, metadata) = core::CoreModule::load(source)
                    .await
                    .map_err(|error| load_error(path, "core load/metadata", error))?;
                Ok((Arc::new(Self::Core(module)), metadata))
            }
        }
    }
    pub async fn create(&self, bridge: HostBridge) -> Result<Arc<WasmInstance>, ApiError> {
        let instance = match self {
            Self::Component(module) => WasmInstance::Component(module.create(bridge).await?),
            Self::Core(module) => WasmInstance::Core(module.create(bridge).await?),
        };
        Ok(Arc::new(instance))
    }
}
impl WasmInstance {
    pub fn is_failed(&self) -> bool {
        match self {
            Self::Component(instance) => instance.is_failed(),
            Self::Core(instance) => instance.is_failed(),
        }
    }
    pub async fn invoke(&self, operation: u32, payload: Vec<u8>) -> Result<Value, ApiError> {
        match self {
            Self::Component(instance) => instance.invoke(operation, payload).await,
            Self::Core(instance) => instance.invoke(operation, payload).await,
        }
    }
}

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "extension",
        imports: { default: trappable },
        exports: { default: async | store },
        with: { "wasi": wasmtime_wasi::p3::bindings },
        require_store_data_send: true,
    });
}

const FUEL: u64 = 100_000_000;
const MEMORY: usize = 128 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);
struct State {
    runtime: Weak<Inner>,
    id: String,
    cancel: CancellationToken,
    calls: Arc<Semaphore>,
    clocks: WasiClocksCtx,
    table: ResourceTable,
    limits: StoreLimits,
}
impl WasiClocksView for State {
    fn clocks(&mut self) -> WasiClocksCtxView<'_> {
        WasiClocksCtxView {
            ctx: &mut self.clocks,
            table: &mut self.table,
        }
    }
}
impl bindings::atri::extension::host::Host for State {}
impl bindings::atri::extension::host::HostWithStore<State> for HasSelf<State> {
    async fn invoke(accessor: &Accessor<State, Self>, frame: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
        let request: Request = decode(&frame).map_err(|e| wasmtime::Error::msg(e.to_string()))?;
        drop(frame);
        let (runtime, id, cancel, calls) = accessor.with(|mut access| {
            let state = access.get();
            (
                state.runtime.upgrade(),
                state.id.clone(),
                state.cancel.clone(),
                state.calls.clone(),
            )
        });
        let _permit = calls
            .try_acquire_owned()
            .map_err(|_| wasmtime::Error::msg("guest concurrent host-call quota exceeded"))?;
        // Discovery never receives a runtime or authenticated caller identity.
        let result = match runtime {
            Some(runtime) => runtime.host(&id, cancel, request).await,
            None => Err(ApiError::new(
                "metadata_host_call",
                "host calls are unavailable while discovering metadata",
            )),
        };
        encode(&result).map_err(|e| wasmtime::Error::msg(e.to_string()))
    }
}
struct Guest {
    store: Store<State>,
    bindings: bindings::Extension,
}
pub(crate) struct ComponentModule {
    engine: Engine,
    component: Component,
    id: String,
}
pub(crate) struct ComponentInstance {
    // Take ownership before awaiting guest code. Dropping a callback future drops
    // the whole store, never an inconsistent guest which could later be re-entered.
    guest: Mutex<Option<Guest>>,
    failed: AtomicBool,
}
fn execution_error(error: wasmtime::Error) -> ApiError {
    ApiError::new("wasm_execution", format!("{error:#}"))
}
fn linker(engine: &Engine) -> Result<Linker<State>, ApiError> {
    let mut linker = Linker::new(engine);
    bindings::atri::extension::host::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
        .map_err(ApiError::internal)?;
    // Deliberately link only the declared P3 clock: no filesystem, networking,
    // environment, P1/P2 compatibility, or ambient process capabilities.
    wasmtime_wasi::p3::bindings::clocks::monotonic_clock::add_to_linker::<_, WasiClocks>(
        &mut linker,
        State::clocks,
    )
    .map_err(ApiError::internal)?;
    Ok(linker)
}
impl ComponentModule {
    async fn load(source: Vec<u8>) -> Result<(Arc<Self>, ExtensionMetadata), ApiError> {
        let mut config = Config::new();
        config.wasm_component_model_async(true).consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|error| ApiError::new("wasm_engine", error.to_string()))?;
        let compiler = engine.clone();
        // Only actual components enter this backend; core modules have their own ABI.
        let component = tokio::task::spawn_blocking(move || Component::new(&compiler, source))
            .await
            .map_err(|error| {
                ApiError::new(
                    "wasm_compile",
                    format!("component compiler task failed: {error}"),
                )
            })?
            .map_err(|error| {
                ApiError::new(
                    "wasm_compile",
                    format!("component compilation failed: {error:#}"),
                )
            })?;
        let mut module = Self {
            engine,
            component,
            id: String::new(),
        };
        let mut guest = module.instantiate().await?;
        let metadata = tokio::time::timeout(
            DEADLINE,
            guest.store.run_concurrent(async |store| {
                guest
                    .bindings
                    .atri_extension_plugin()
                    .call_describe(store)
                    .await
            }),
        )
        .await
        .map_err(|_| ApiError::new("wasm_timeout", "guest metadata exceeded deadline"))?
        .map_err(execution_error)?
        .map_err(execution_error)?;
        let metadata = decode_response(&metadata)?;
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
            calls: Arc::new(Semaphore::new(64)),
            clocks: WasiClocksCtx::default(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY)
                .memories(1)
                .tables(4)
                .table_elements(100_000)
                .instances(16)
                .build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(FUEL).map_err(ApiError::internal)?;
        store
            .fuel_async_yield_interval(Some(100_000))
            .map_err(ApiError::internal)?;
        let bindings = tokio::time::timeout(
            Duration::from_secs(10),
            bindings::Extension::instantiate_async(
                &mut store,
                &self.component,
                &linker(&self.engine)?,
            ),
        )
        .await
        .map_err(|_| ApiError::new("wasm_timeout", "guest instantiation exceeded deadline"))?
        .map_err(execution_error)?;
        Ok(Guest { store, bindings })
    }
    pub async fn create(&self, bridge: HostBridge) -> Result<Arc<ComponentInstance>, ApiError> {
        if bridge.id != self.id {
            return Err(ApiError::new(
                "extension_id_mismatch",
                "host caller identity differs from discovered metadata",
            ));
        }
        let mut guest = self.instantiate().await?;
        let state = guest.store.data_mut();
        state.runtime = bridge.runtime;
        state.id = bridge.id;
        state.cancel = bridge.cancel;
        Ok(Arc::new(ComponentInstance {
            guest: Mutex::new(Some(guest)),
            failed: AtomicBool::new(false),
        }))
    }
}
fn decode_response(bytes: &[u8]) -> Result<Value, ApiError> {
    match decode::<Result<Value, ApiError>>(bytes)? {
        Err(error) if error.code.len().saturating_add(error.message.len()) > 64 * 1024 => {
            Err(ApiError::new(
                "output_too_large",
                "guest error exceeds the 64 KiB diagnostic budget",
            ))
        }
        result => result,
    }
}
impl ComponentInstance {
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
            // A failed component owns no live guest state. Stop is disposal, not
            // an attempt to call back into a trapped or abandoned component.
            return if operation == wasm_abi::STOP {
                Ok(Value::Null)
            } else {
                Err(ApiError::new(
                    "wasm_failed",
                    "component was discarded after failed execution",
                ))
            };
        };
        self.failed.store(true, Ordering::Release);
        guest.store.set_fuel(FUEL).map_err(ApiError::internal)?;
        let cancel = guest.store.data().cancel.clone();
        let invocation = guest.store.run_concurrent(async |store| {
            let plugin = guest.bindings.atri_extension_plugin();
            match operation {
                wasm_abi::START => plugin.call_start(store).await,
                wasm_abi::EVENT => plugin.call_on_event(store, payload).await,
                wasm_abi::SERVICE => plugin.call_on_service(store, payload).await,
                wasm_abi::STOP => plugin.call_stop(store).await,
                _ => Err(wasmtime::Error::msg("unsupported component callback")),
            }
        });
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled(), if operation != wasm_abi::STOP => return Err(ApiError::cancelled()),
            result = tokio::time::timeout(DEADLINE, invocation) => result,
        };
        let bytes = result
            .map_err(|_| ApiError::new("wasm_timeout", "guest invocation exceeded deadline"))?
            .map_err(execution_error)?
            .map_err(execution_error)?;
        // A returned API error is not a trap: the component completed normally.
        self.failed.store(false, Ordering::Release);
        *slot = Some(guest);
        decode_response(&bytes)
    }
}
