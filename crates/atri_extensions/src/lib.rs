//! Platform-independent extension APIs. All configuration and storage I/O belongs to the host.

extern crate self as atri_extensions;

pub use async_trait::async_trait;
pub use atri_core::*;
pub use atri_macros::extension;
mod api;
pub use api::{Account, Conversation, ServiceRouter};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod native;
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod wasm;
#[cfg(not(target_arch = "wasm32"))]
pub use native::ManagedTask;
#[cfg(not(target_arch = "wasm32"))]
pub use tokio_util::sync::CancellationToken;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Extension: Send + Sync + 'static {
    /// Return stable metadata without performing I/O.
    fn metadata() -> ExtensionMetadata
    where
        Self: Sized;

    async fn start(&self, _ctx: Context) -> Result<(), ApiError> {
        Ok(())
    }
    async fn on_event(&self, _ctx: Context, _event: Event) -> Result<EventDisposition, ApiError> {
        Ok(EventDisposition::Continue)
    }
    async fn on_service(&self, _ctx: Context, call: ServiceCall) -> Result<Value, ApiError> {
        Err(ApiError::new(
            "unknown_method",
            format!("unknown service method {}/{}", call.service, call.method),
        ))
    }
    async fn stop(&self, _ctx: Context) -> Result<(), ApiError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct Context {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) inner: native::NativeContext,
}
impl Context {
    pub async fn invoke<T, R>(&self, method: &str, params: &T) -> Result<R, ApiError>
    where
        T: Serialize + Sync + ?Sized,
        R: DeserializeOwned,
    {
        let request = Request {
            method: method.into(),
            params: to_value(params)?,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let value = self.inner.invoke(request).await?;
        #[cfg(target_arch = "wasm32")]
        let value = wasm::host_call(request).await?;
        from_value(&value)
    }

    pub fn config(&self) -> Config {
        Config { ctx: self.clone() }
    }
    pub fn storage(&self) -> Storage {
        Storage { ctx: self.clone() }
    }
    /// Return retained lifecycle state, including accounts published before this extension started.
    /// Requires the `bots.read` capability.
    pub async fn bots(&self) -> Result<Vec<BotState>, ApiError> {
        self.invoke("bots.list", &()).await
    }

    pub async fn emit<T: Serialize + Sync + ?Sized>(
        &self,
        kind: EventKind,
        account: Option<&str>,
        payload: &T,
    ) -> Result<(), ApiError> {
        self.invoke(
            "events.emit",
            &PublishEvent {
                kind,
                account: account.map(str::to_owned),
                payload: to_value(payload)?,
            },
        )
        .await
    }
    pub async fn register_service(
        &self,
        name: &str,
        account: Option<&str>,
    ) -> Result<(), ApiError> {
        self.invoke(
            "services.register",
            &ServiceBinding {
                service: name.into(),
                account: account.map(str::to_owned),
            },
        )
        .await
    }
    pub async fn unregister_service(
        &self,
        name: &str,
        account: Option<&str>,
    ) -> Result<(), ApiError> {
        self.invoke(
            "services.unregister",
            &ServiceBinding {
                service: name.into(),
                account: account.map(str::to_owned),
            },
        )
        .await
    }
    pub async fn log(&self, level: LogLevel, message: &str) -> Result<(), ApiError> {
        self.invoke(
            "log",
            &LogRecord {
                level,
                message: message.into(),
                fields: BTreeMap::new(),
            },
        )
        .await
    }
    pub async fn output_table(&self, table: OutputTable) -> Result<(), ApiError> {
        self.invoke("output.table", &table).await
    }
    pub async fn interaction(&self, request: InteractionRequest) -> Result<(), ApiError> {
        self.invoke("interaction", &request).await
    }
    pub async fn sleep(&self, duration: Duration) -> Result<(), ApiError> {
        #[cfg(target_arch = "wasm32")]
        {
            wasm::sleep(duration).await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let milliseconds = u64::try_from(duration.as_millis()).map_err(|_| {
                ApiError::new(
                    "invalid_duration",
                    "sleep duration exceeds u64 milliseconds",
                )
            })?;
            self.invoke("tasks.sleep", &SleepRequest { milliseconds })
                .await
        }
    }
    pub async fn paths(&self) -> Result<ExtensionPaths, ApiError> {
        self.invoke("paths", &()).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn<F>(&self, name: impl Into<String>, future: F) -> Result<ManagedTask, ApiError>
    where
        F: Future<Output = Result<(), ApiError>> + Send + 'static,
    {
        self.inner.spawn(name.into(), future)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_with_timeout<F>(
        &self,
        name: impl Into<String>,
        timeout: Duration,
        future: F,
    ) -> Result<ManagedTask, ApiError>
    where
        F: Future<Output = Result<(), ApiError>> + Send + 'static,
    {
        self.inner.spawn_with_timeout(name.into(), timeout, future)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancellation.clone()
    }
}

#[derive(Clone)]
pub struct Config {
    ctx: Context,
}
impl Config {
    pub async fn load_or_create<T>(&self, defaults: &T) -> Result<T, ApiError>
    where
        T: Serialize + DeserializeOwned + Sync,
    {
        let snapshot: ConfigSnapshot = self
            .ctx
            .invoke(
                "config.load",
                &ConfigLoad {
                    defaults: Some(to_value(defaults)?),
                    format: None,
                },
            )
            .await?;
        from_value(&snapshot.value)
    }
    pub async fn load<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        let snapshot: ConfigSnapshot = self
            .ctx
            .invoke(
                "config.load",
                &ConfigLoad {
                    defaults: None,
                    format: None,
                },
            )
            .await?;
        from_value(&snapshot.value)
    }
    pub async fn save<T: Serialize + Sync + ?Sized>(&self, value: &T) -> Result<(), ApiError> {
        self.store(value, None).await.map(|_| ())
    }
    pub async fn store<T: Serialize + Sync + ?Sized>(
        &self,
        value: &T,
        expected_revision: Option<&str>,
    ) -> Result<ConfigSnapshot, ApiError> {
        self.ctx
            .invoke(
                "config.store",
                &ConfigStore {
                    value: to_value(value)?,
                    expected_revision: expected_revision.map(str::to_owned),
                    format: None,
                },
            )
            .await
    }
    pub async fn read_document(&self) -> Result<ConfigDocument, ApiError> {
        self.ctx.invoke("config.document.read", &()).await
    }
    pub async fn write_document(
        &self,
        document: ConfigDocumentStore,
    ) -> Result<ConfigDocument, ApiError> {
        self.ctx.invoke("config.document.write", &document).await
    }
}

#[derive(Clone)]
pub struct Storage {
    ctx: Context,
}
impl Storage {
    pub async fn read(&self, area: &str, path: &str) -> Result<Vec<u8>, ApiError> {
        validate_area(area)?;
        let bytes: serde_bytes::ByteBuf = self
            .ctx
            .invoke(
                "storage.read",
                &StorageRead {
                    area: area.into(),
                    path: path.into(),
                },
            )
            .await?;
        Ok(bytes.into_vec())
    }
    pub async fn write(&self, area: &str, path: &str, bytes: &[u8]) -> Result<(), ApiError> {
        validate_area(area)?;
        #[derive(Serialize)]
        struct Write<'a> {
            area: &'a str,
            path: &'a str,
            #[serde(with = "serde_bytes")]
            bytes: &'a [u8],
        }
        self.ctx
            .invoke("storage.write", &Write { area, path, bytes })
            .await
    }
}
fn validate_area(area: &str) -> Result<(), ApiError> {
    match area {
        "data" | "workspace" => Ok(()),
        _ => Err(ApiError::new(
            "invalid_storage_area",
            "storage area must be data or workspace",
        )),
    }
}

pub(crate) fn validate_service<E: Extension>(call: &ServiceCall) -> Result<(), ApiError> {
    let metadata = E::metadata();
    let service = metadata
        .services
        .iter()
        .find(|service| service.name == call.service)
        .ok_or_else(|| {
            ApiError::new(
                "unknown_service",
                format!("undeclared service {}", call.service),
            )
        })?;
    if !service.methods.iter().any(|method| method == &call.method) {
        return Err(ApiError::new(
            "unknown_method",
            format!("undeclared method {}/{}", call.service, call.method),
        ));
    }
    Ok(())
}

#[cfg(all(target_arch = "wasm32", not(feature = "core-module")))]
#[doc(hidden)]
#[macro_export]
macro_rules! __export_guest_extension {
    ($extension:ty) => {
        const _: () = {
            struct Component;
            impl $crate::wasm::bindings::exports::atri::extension::plugin::Guest for Component {
                fn describe() -> Vec<u8> {
                    $crate::wasm::describe::<$extension>()
                }
                async fn start() -> Vec<u8> {
                    $crate::wasm::start::<$extension>().await
                }
                async fn on_event(event: Vec<u8>) -> Vec<u8> {
                    $crate::wasm::on_event::<$extension>(&event).await
                }
                async fn on_service(call: Vec<u8>) -> Vec<u8> {
                    $crate::wasm::on_service::<$extension>(&call).await
                }
                async fn stop() -> Vec<u8> {
                    $crate::wasm::stop().await
                }
            }
    $crate::wasm::bindings::export!(Component with_types_in $crate::wasm::bindings);
        };
    };
}

#[cfg(all(target_arch = "wasm32", feature = "core-module"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __export_guest_extension {
    ($extension:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn atri_abi_version() -> u32 {
            $crate::API_VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn atri_alloc(len: u32) -> u32 {
            $crate::wasm::core::allocate(len)
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn atri_free(ptr: u32, len: u32) {
            unsafe { $crate::wasm::core::release(ptr, len) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn atri_invoke(op: u32, ptr: u32, len: u32) -> u64 {
            unsafe { $crate::wasm::core::invoke::<$extension>(op, ptr, len) }
        }
    };
}
