use crate::{Context, Extension, validate_service};
use atri_core::*;
use std::cell::Cell;
#[cfg(not(feature = "core-module"))]
use std::time::Duration;

#[cfg(feature = "core-module")]
pub mod core;
#[cfg(feature = "core-module")]
pub(crate) use core::{host_call, sleep};

/// Shared component bindings. Async WIT functions retain the async canonical ABI.
#[cfg(not(feature = "core-module"))]
pub mod bindings {
    wit_bindgen::generate!({
        world: "extension",
        path: "../../wit",
        pub_export_macro: true,
        generate_all,
        default_bindings_module: "atri_extensions::wasm::bindings",
    });
}

#[cfg(not(feature = "core-module"))]
pub(crate) async fn host_call(request: Request) -> Result<Value, ApiError> {
    let bytes = encode(&request)?;
    let response = bindings::atri::extension::host::invoke(bytes).await;
    decode::<Result<Value, ApiError>>(&response)?
}

#[cfg(not(feature = "core-module"))]
pub(crate) async fn sleep(duration: Duration) -> Result<(), ApiError> {
    let nanoseconds = u64::try_from(duration.as_nanos())
        .map_err(|_| ApiError::new("invalid_duration", "sleep duration exceeds u64 nanoseconds"))?;
    bindings::wasi::clocks::monotonic_clock::wait_for(nanoseconds).await;
    Ok(())
}

enum Slot {
    Empty,
    Ready(Box<dyn Extension>),
    Busy,
}

thread_local! {
    // The singleton is owned by the component, not leaked. Cell transfers that
    // ownership to each hook without retaining a RefCell guard across guest
    // code, suspension, cancellation, or a Wasm trap. A trapped component is
    // discarded by the host; its entire linear memory is then reclaimed.
    static INSTANCE: Cell<Slot> = const { Cell::new(Slot::Empty) };
}

struct Invocation {
    extension: Option<Box<dyn Extension>>,
    retain: bool,
}
impl Invocation {
    fn acquire() -> Result<Self, ApiError> {
        INSTANCE.with(|slot| match slot.replace(Slot::Busy) {
            Slot::Ready(extension) => Ok(Self {
                extension: Some(extension),
                retain: true,
            }),
            previous => {
                let busy = matches!(previous, Slot::Busy);
                slot.set(previous);
                Err(if busy {
                    ApiError::new(
                        "concurrent_invocation",
                        "component hooks must be serialized",
                    )
                } else {
                    ApiError::new("not_started", "component has not started")
                })
            }
        })
    }

    fn begin<E: Extension + Default>() -> Result<Self, ApiError> {
        INSTANCE
            .with(|slot| match slot.replace(Slot::Busy) {
                Slot::Empty => Ok(Self {
                    extension: None,
                    retain: false,
                }),
                previous => {
                    slot.set(previous);
                    Err(ApiError::new(
                        "already_started",
                        "component already started or is busy",
                    ))
                }
            })
            .map(|mut invocation| {
                invocation.extension = Some(Box::new(E::default()));
                invocation
            })
    }

    fn extension(&self) -> &dyn Extension {
        self.extension
            .as_deref()
            .expect("invocation owns its extension")
    }
}
impl Drop for Invocation {
    fn drop(&mut self) {
        let next = if self.retain {
            self.extension.take().map_or(Slot::Empty, Slot::Ready)
        } else {
            Slot::Empty
        };
        INSTANCE.with(|slot| slot.set(next));
    }
}

fn metadata<E: Extension>() -> Result<ExtensionMetadata, ApiError> {
    let metadata = E::metadata();
    if metadata.id.is_empty() || !metadata.id.is_ascii() {
        return Err(ApiError::new(
            "invalid_extension_id",
            "extension metadata id must be nonempty ASCII",
        ));
    }
    Ok(metadata)
}

fn response(result: Result<Value, ApiError>) -> Vec<u8> {
    match encode(&result) {
        Ok(bytes) => bytes,
        Err(error) => encode(&Err::<Value, ApiError>(error)).expect("ApiError is serializable"),
    }
}

pub fn describe<E: Extension>() -> Vec<u8> {
    response(metadata::<E>().and_then(|metadata| to_value(&metadata)))
}

pub async fn start<E: Extension + Default>() -> Vec<u8> {
    response(
        async {
            metadata::<E>()?;
            let mut invocation = Invocation::begin::<E>()?;
            invocation.extension().start(Context {}).await?;
            invocation.retain = true;
            Ok(Value::Null)
        }
        .await,
    )
}

pub async fn on_event<E: Extension>(bytes: &[u8]) -> Vec<u8> {
    response(
        async {
            let event: Event = decode(bytes)?;
            let invocation = Invocation::acquire()?;
            to_value(&invocation.extension().on_event(Context {}, event).await?)
        }
        .await,
    )
}

pub async fn on_service<E: Extension>(bytes: &[u8]) -> Vec<u8> {
    response(
        async {
            let call: ServiceCall = decode(bytes)?;
            validate_service::<E>(&call)?;
            let invocation = Invocation::acquire()?;
            invocation.extension().on_service(Context {}, call).await
        }
        .await,
    )
}

pub async fn stop() -> Vec<u8> {
    response(
        async {
            let mut invocation = Invocation::acquire()?;
            invocation.extension().stop(Context {}).await?;
            invocation.retain = false;
            Ok(Value::Null)
        }
        .await,
    )
}
