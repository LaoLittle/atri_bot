use crate::{Context, Extension, validate_service};
use async_ffi::{FfiFuture, FutureExt as FfiFutureExt};
use atri_core::{abi::*, *};
use futures::{FutureExt, channel::oneshot, future::BoxFuture};
use std::{
    collections::{BTreeMap, hash_map::RandomState},
    hash::BuildHasher,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
mod diagnostics;

const NATIVE_TASK_BIT: u64 = 1 << 63;
const TASK_SERVICE: &str = "atri.sdk.tasks";

#[derive(Clone)]
pub(crate) struct NativeContext {
    scope: Arc<Scope>,
    pub(crate) cancellation: CancellationToken,
}
impl NativeContext {
    pub(crate) async fn invoke(&self, request: Request) -> Result<Value, ApiError> {
        if self.cancellation.is_cancelled() {
            return Err(ApiError::cancelled());
        }
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(ApiError::cancelled()),
            result = invoke_host(self.scope.host, request) => result,
        }
    }
    pub(crate) fn spawn<F>(&self, name: String, future: F) -> Result<ManagedTask, ApiError>
    where
        F: Future<Output = Result<(), ApiError>> + Send + 'static,
    {
        self.enqueue_task(name, Box::pin(future), None)
    }
    pub(crate) fn spawn_with_timeout<F>(
        &self,
        name: String,
        timeout: Duration,
        future: F,
    ) -> Result<ManagedTask, ApiError>
    where
        F: Future<Output = Result<(), ApiError>> + Send + 'static,
    {
        self.enqueue_task(name, Box::pin(future), Some(timeout))
    }
    fn enqueue_task(
        &self,
        name: String,
        future: BoxFuture<'static, Result<(), ApiError>>,
        timeout: Option<Duration>,
    ) -> Result<ManagedTask, ApiError> {
        let mut tasks = self.scope.tasks.lock().map_err(ApiError::internal)?;
        if self.scope.stopping.load(Ordering::Acquire) || self.cancellation.is_cancelled() {
            return Err(ApiError::cancelled());
        }
        let id = loop {
            let sequence = self.scope.next_id.fetch_add(1, Ordering::Relaxed);
            if sequence >= NATIVE_TASK_BIT {
                return Err(ApiError::new(
                    "task_id_exhausted",
                    "native task identifiers exhausted",
                ));
            }
            let id = NATIVE_TASK_BIT
                | self
                    .scope
                    .id_seed
                    .hash_one((self.scope.host.context, sequence));
            if !tasks.contains_key(&id) {
                break id;
            }
        };
        let cancellation = self.scope.cancellation.child_token();
        let (done, result) = watch::channel(None);
        tasks.insert(id, cancellation.clone());
        let command = Command::Spawn {
            id,
            name,
            future,
            timeout,
            cancellation: cancellation.clone(),
            done,
        };
        if self.scope.sender.send(command).is_err() {
            tasks.remove(&id);
            return Err(worker_stopped());
        }
        Ok(ManagedTask {
            id,
            cancellation,
            result,
        })
    }
}

/// A task owned by the extension scope, independent of the callback that spawned it.
/// Dropping a handle detaches observation, not scope ownership; stop still cancels and joins it.
#[derive(Clone)]
pub struct ManagedTask {
    id: u64,
    cancellation: CancellationToken,
    result: watch::Receiver<Option<Result<(), ApiError>>>,
}
impl ManagedTask {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn abort(&self) {
        self.cancellation.cancel();
    }
    pub async fn join(&self) -> Result<(), ApiError> {
        let mut result = self.result.clone();
        loop {
            if let Some(outcome) = result.borrow_and_update().clone() {
                return outcome;
            }
            result.changed().await.map_err(|_| worker_stopped())?;
        }
    }
}

struct Scope {
    host: HostApi,
    sender: mpsc::UnboundedSender<Command>,
    cancellation: CancellationToken,
    stopping: AtomicBool,
    finished: AtomicBool,
    next_id: AtomicU64,
    id_seed: RandomState,
    tasks: Mutex<BTreeMap<u64, CancellationToken>>,
}
impl Scope {
    fn context(self: &Arc<Self>, cancellation: CancellationToken) -> Context {
        Context {
            inner: NativeContext {
                scope: self.clone(),
                cancellation,
            },
        }
    }
    fn cancel_task(&self, id: u64) -> Result<(), ApiError> {
        let tasks = self.tasks.lock().map_err(ApiError::internal)?;
        let task = tasks
            .get(&id)
            .ok_or_else(|| ApiError::new("unknown_task", format!("unknown native task {id}")))?;
        task.cancel();
        Ok(())
    }
}
struct Instance {
    scope: Arc<Scope>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
enum Command {
    Call {
        operation: u32,
        payload: OwnedBuffer,
        reply: oneshot::Sender<AbiResult>,
        cancellation: CancellationToken,
    },
    Spawn {
        id: u64,
        name: String,
        future: BoxFuture<'static, Result<(), ApiError>>,
        timeout: Option<Duration>,
        cancellation: CancellationToken,
        done: watch::Sender<Option<Result<(), ApiError>>>,
    },
    Stop {
        reply: oneshot::Sender<AbiResult>,
    },
}

async fn invoke_host(host: HostApi, request: Request) -> Result<Value, ApiError> {
    let bytes = encode(&request)?;
    // The host callback enqueues before returning. Its future must only wait for completion.
    unsafe { (host.invoke)(host.context, OwnedBuffer::from_vec(bytes)) }
        .await
        .into_result()
}
async fn task_notice(
    host: HostApi,
    method: &str,
    id: u64,
    name: &str,
    state: &str,
) -> Result<(), ApiError> {
    let request = Request::new(
        method,
        TaskRecord {
            id,
            name: name.into(),
            state: state.into(),
        },
    )?;
    invoke_host(host, request).await.map(|_| ())
}
fn worker_stopped() -> ApiError {
    ApiError::new("worker_stopped", "extension executor is no longer running")
}
fn panic_error() -> ApiError {
    ApiError::new("extension_panicked", "extension callback panicked")
}

// This guard lives inside the FFI future, not on either Tokio executor. Dropping a
// caller's completion future cancels the corresponding extension callback.
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn completion(
    receiver: oneshot::Receiver<AbiResult>,
    cancellation: Option<CancellationToken>,
) -> FfiFuture<AbiResult> {
    let guard = cancellation.map(CancelOnDrop);
    async move {
        let _guard = guard;
        receiver
            .await
            .unwrap_or_else(|_| AbiResult::error(worker_stopped()))
    }
    .into_ffi()
}
fn immediate(error: ApiError) -> FfiFuture<AbiResult> {
    async move { AbiResult::error(error) }.into_ffi()
}

pub const fn exports<E: Extension + Default>() -> NativeExtensionV1 {
    NativeExtensionV1 {
        abi_version: API_VERSION,
        struct_size: std::mem::size_of::<NativeExtensionV1>() as u32,
        future_abi_version: FUTURE_ABI_VERSION,
        metadata: metadata::<E>,
        create: create::<E>,
        start,
        event,
        service,
        stop,
        destroy,
    }
}
unsafe extern "C" fn metadata<E: Extension>() -> AbiResult {
    match catch_unwind(AssertUnwindSafe(|| {
        let metadata = E::metadata();
        if metadata.id.is_empty() || !metadata.id.is_ascii() {
            return Err(ApiError::new(
                "invalid_extension_id",
                "extension metadata id must be nonempty ASCII",
            ));
        }
        Ok(metadata)
    })) {
        Ok(Ok(metadata)) => AbiResult::ok(&metadata),
        Ok(Err(error)) => AbiResult::error(error),
        Err(_) => AbiResult::error(panic_error()),
    }
}
unsafe extern "C" fn create<E: Extension + Default>(host: HostApi) -> InstanceResult {
    match create_instance::<E>(host) {
        Ok(instance) => InstanceResult {
            instance: Box::into_raw(Box::new(instance)) as usize,
            error: OwnedBuffer::empty(),
        },
        Err(error) => InstanceResult {
            instance: 0,
            error: OwnedBuffer::from_vec(encode(&error).expect("ApiError is serializable")),
        },
    }
}
fn create_instance<E: Extension + Default>(host: HostApi) -> Result<Instance, ApiError> {
    host.validate()?;
    let (sender, receiver) = mpsc::unbounded_channel();
    let scope = Arc::new(Scope {
        host,
        sender,
        cancellation: CancellationToken::new(),
        stopping: AtomicBool::new(false),
        finished: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        tasks: Mutex::new(BTreeMap::new()),
        id_seed: RandomState::new(),
    });
    let worker_scope = scope.clone();
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("atri-extension".into())
        .spawn(move || {
            let _diagnostics = diagnostics::ThreadGuard::enter(host);
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .on_thread_start(move || diagnostics::register(host))
                .on_thread_stop(diagnostics::exit)
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_sender.send(Err(ApiError::internal(error)));
                    return;
                }
            };
            let extension = {
                let _entered = runtime.enter();
                match catch_unwind(AssertUnwindSafe(|| {
                    diagnostics::during(wasm_abi::START, 0, E::default)
                })) {
                    Ok(extension) => Arc::new(extension),
                    Err(_) => {
                        let _ = ready_sender.send(Err(panic_error()));
                        return;
                    }
                }
            };
            if ready_sender.send(Ok(())).is_err() {
                return;
            }
            let (reply, result) =
                runtime.block_on(run_worker(extension, worker_scope.clone(), receiver));
            // Runtime destruction (including any library-created blocking jobs) happens
            // on this dedicated OS thread, never inside the caller's async executor.
            drop(runtime);
            worker_scope.finished.store(true, Ordering::Release);
            if let Some(reply) = reply {
                let _ = reply.send(AbiResult::from_result(result));
            }
        })
        .map_err(ApiError::internal)?;
    match ready.recv().map_err(ApiError::internal)? {
        Ok(()) => Ok(Instance {
            scope,
            worker: Mutex::new(Some(worker)),
        }),
        Err(error) => {
            let _ = worker.join();
            Err(error)
        }
    }
}
unsafe fn instance<'a>(pointer: usize) -> Result<&'a Instance, ApiError> {
    if pointer == 0 {
        return Err(ApiError::new("invalid_instance", "null extension instance"));
    }
    Ok(unsafe { &*(pointer as *const Instance) })
}
unsafe fn enqueue(pointer: usize, operation: u32, payload: OwnedBuffer) -> FfiFuture<AbiResult> {
    let instance = match unsafe { instance(pointer) } {
        Ok(instance) => instance,
        Err(error) => return immediate(error),
    };
    let _tasks = match instance.scope.tasks.lock() {
        Ok(tasks) => tasks,
        Err(error) => return immediate(ApiError::internal(error)),
    };
    if instance.scope.stopping.load(Ordering::Acquire) {
        return immediate(ApiError::cancelled());
    }
    let cancellation = instance.scope.cancellation.child_token();
    let (reply, receiver) = oneshot::channel();
    let command = Command::Call {
        operation,
        payload,
        reply,
        cancellation: cancellation.clone(),
    };
    if instance.scope.sender.send(command).is_err() {
        return immediate(worker_stopped());
    }
    completion(receiver, Some(cancellation))
}
unsafe extern "C" fn start(pointer: usize) -> FfiFuture<AbiResult> {
    unsafe { enqueue(pointer, wasm_abi::START, OwnedBuffer::empty()) }
}
unsafe extern "C" fn event(pointer: usize, payload: OwnedBuffer) -> FfiFuture<AbiResult> {
    unsafe { enqueue(pointer, wasm_abi::EVENT, payload) }
}
unsafe extern "C" fn service(pointer: usize, payload: OwnedBuffer) -> FfiFuture<AbiResult> {
    unsafe { enqueue(pointer, wasm_abi::SERVICE, payload) }
}
unsafe extern "C" fn stop(pointer: usize) -> FfiFuture<AbiResult> {
    let instance = match unsafe { instance(pointer) } {
        Ok(instance) => instance,
        Err(error) => return immediate(error),
    };
    let _tasks = match instance.scope.tasks.lock() {
        Ok(tasks) => tasks,
        Err(error) => return immediate(ApiError::internal(error)),
    };
    if instance.scope.stopping.swap(true, Ordering::AcqRel) {
        return immediate(ApiError::new(
            "already_stopping",
            "extension stop already requested",
        ));
    }
    instance.scope.cancellation.cancel();
    let (reply, receiver) = oneshot::channel();
    if instance.scope.sender.send(Command::Stop { reply }).is_err() {
        return immediate(worker_stopped());
    }
    // A dropped stop future cannot abandon shutdown or kill the worker midway.
    completion(receiver, None)
}
unsafe extern "C" fn destroy(pointer: usize) {
    let Ok(instance) = (unsafe { instance(pointer) }) else {
        return;
    };
    if !instance.scope.finished.load(Ordering::Acquire) {
        return;
    }
    let instance = unsafe { Box::from_raw(pointer as *mut Instance) };
    let worker = instance
        .worker
        .lock()
        .ok()
        .and_then(|mut worker| worker.take());
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

async fn run_worker<E: Extension>(
    extension: Arc<E>,
    scope: Arc<Scope>,
    mut receiver: mpsc::UnboundedReceiver<Command>,
) -> (Option<oneshot::Sender<AbiResult>>, Result<(), ApiError>) {
    let mut jobs = tokio::task::JoinSet::new();
    let stop_reply = loop {
        tokio::select! {
            command = receiver.recv() => match command {
                Some(Command::Call { operation, payload, reply, cancellation }) => {
                    let extension = extension.clone();
                    let ctx = scope.context(scope.cancellation.clone());
                    let scope = scope.clone();
                    jobs.spawn(async move {
                        let callback = AssertUnwindSafe(diagnostics::track(operation, 0,
                            dispatch(extension.as_ref(), ctx, &scope, operation, payload))).catch_unwind();
                        let result = tokio::select! {
                            biased;
                            _ = cancellation.cancelled() => Err(ApiError::cancelled()),
                            result = callback => result.unwrap_or_else(|_| Err(panic_error())),
                        };
                        let _ = reply.send(AbiResult::from_result(result));
                    });
                }
                Some(Command::Spawn { id, name, future, timeout, cancellation, done }) => {
                    let scope = scope.clone();
                    jobs.spawn(async move {
                        let body = async {
                            task_notice(scope.host, "tasks.started", id, &name, "running").await?;
                            let guarded = AssertUnwindSafe(diagnostics::track(5, id, future)).catch_unwind();
                            match timeout {
                                Some(timeout) => tokio::time::timeout(timeout, guarded).await
                                    .map_err(|_| ApiError::new("deadline_exceeded", "managed task exceeded its deadline"))?
                                    .unwrap_or_else(|_| Err(panic_error())),
                                None => guarded.await.unwrap_or_else(|_| Err(panic_error())),
                            }
                        };
                        let result = tokio::select! {
                            biased;
                            _ = cancellation.cancelled() => Err(ApiError::cancelled()),
                            result = body => result,
                        };
                        let state = match &result {
                            Ok(()) => "completed", Err(error) if error.code == "cancelled" => "cancelled",
                            Err(error) if error.code == "deadline_exceeded" => "timeout", Err(_) => "failed",
                        };
                        // Completion notices are cleanup operations and must survive scope cancellation.
                        let notice = task_notice(scope.host, "tasks.finished", id, &name, state).await;
                        let result = result.and(notice);
                        if let Ok(mut tasks) = scope.tasks.lock() { tasks.remove(&id); }
                        done.send_replace(Some(result));
                    });
                }
                Some(Command::Stop { reply }) => break Some(reply),
                None => break None,
            },
            _ = jobs.join_next(), if !jobs.is_empty() => {},
        }
    };
    receiver.close();
    scope.stopping.store(true, Ordering::Release);
    scope.cancellation.cancel();
    while jobs.join_next().await.is_some() {}
    // Stop gets an uncancelled cleanup context. No new managed jobs may start.
    let ctx = scope.context(CancellationToken::new());
    let result = AssertUnwindSafe(diagnostics::track(wasm_abi::STOP, 0, extension.stop(ctx)))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(panic_error()));
    (stop_reply, result)
}

async fn dispatch<E: Extension>(
    extension: &E,
    ctx: Context,
    scope: &Scope,
    operation: u32,
    payload: OwnedBuffer,
) -> Result<Value, ApiError> {
    match operation {
        wasm_abi::START => {
            extension.start(ctx).await?;
            Ok(Value::Null)
        }
        wasm_abi::EVENT => {
            let event = decode(payload.as_slice()?)?;
            drop(payload);
            to_value(&extension.on_event(ctx, event).await?)
        }
        wasm_abi::SERVICE => {
            let call: ServiceCall = decode(payload.as_slice()?)?;
            drop(payload);
            if call.service == TASK_SERVICE {
                if call.method != "cancel" {
                    return Err(ApiError::new(
                        "unknown_method",
                        "unknown SDK task service method",
                    ));
                }
                #[derive(serde::Deserialize)]
                struct Cancel {
                    id: u64,
                }
                let request: Cancel = from_value(&call.args)?;
                scope.cancel_task(request.id)?;
                return Ok(Value::Null);
            }
            validate_service::<E>(&call)?;
            extension.on_service(ctx, call).await
        }
        _ => Err(ApiError::new(
            "unknown_operation",
            "unknown native extension operation",
        )),
    }
}

#[cfg(test)]
mod tests;
