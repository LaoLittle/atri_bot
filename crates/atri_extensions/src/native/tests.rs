use super::*;
use crate::async_trait;
use std::sync::Condvar;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[path = "api_tests.rs"]
mod api_tests;

#[derive(Default)]
struct HostState {
    notices: Mutex<Vec<TaskRecord>>,
    threads: Mutex<Vec<(u64, Arc<ThreadActivity>)>>,
    panics: Mutex<Vec<String>>,
    api: api_tests::ApiState,
}
unsafe extern "C" fn host_invoke(pointer: usize, bytes: OwnedBuffer) -> FfiFuture<AbiResult> {
    let state = unsafe { &*(pointer as *const HostState) };
    let result = (|| {
        let request: Request = decode(bytes.as_slice()?)?;
        match request.method.as_str() {
            "tasks.started" | "tasks.finished" => {
                state
                    .notices
                    .lock()
                    .unwrap()
                    .push(from_value(&request.params)?);
                Ok(Value::Null)
            }
            "log" => {
                let _: LogRecord = from_value(&request.params)?;
                Ok(Value::Null)
            }
            "services.call" => state.api.invoke(from_value(&request.params)?),
            method => Err(ApiError::new("unknown_method", method)),
        }
    })();
    // A ready completion contains no Tokio operation, exactly like a queued
    // host completion. Tests intentionally poll this from futures::executor.
    async move { AbiResult::from_result(result) }.into_ffi()
}
unsafe extern "C" fn host_register_thread(pointer: usize, thread_id: u64) -> *const ThreadActivity {
    let state = unsafe { &*(pointer as *const HostState) };
    let slot = Arc::<ThreadActivity>::default();
    let pointer = Arc::as_ptr(&slot);
    state.threads.lock().unwrap().push((thread_id, slot));
    pointer
}
unsafe extern "C" fn host_report_panic(pointer: usize, _: u64, bytes: OwnedBuffer) {
    let state = unsafe { &*(pointer as *const HostState) };
    state
        .panics
        .lock()
        .unwrap()
        .push(String::from_utf8(bytes.as_slice().unwrap().to_vec()).unwrap());
}
struct Fixture {
    api: NativeExtensionV1,
    pointer: usize,
    host: Box<HostState>,
}
impl Fixture {
    fn new<E: Extension + Default>() -> Self {
        let host = Box::<HostState>::default();
        let api = exports::<E>();
        api.validate().unwrap();
        let created = unsafe {
            (api.create)(HostApi {
                abi_version: API_VERSION,
                struct_size: std::mem::size_of::<HostApi>() as u32,
                context: host.as_ref() as *const HostState as usize,
                invoke: host_invoke,
                register_thread: host_register_thread,
                report_panic: host_report_panic,
            })
        };
        assert_ne!(
            created.instance,
            0,
            "{}",
            String::from_utf8_lossy(created.error.as_slice().unwrap())
        );
        Self {
            api,
            pointer: created.instance,
            host,
        }
    }
    fn start(&self) -> Result<(), ApiError> {
        futures::executor::block_on(unsafe { (self.api.start)(self.pointer) }).into_result()
    }
    fn call(&self, call: ServiceCall) -> Result<Value, ApiError> {
        futures::executor::block_on(unsafe {
            (self.api.service)(self.pointer, OwnedBuffer::from_vec(encode(&call).unwrap()))
        })
        .into_result()
    }
    fn stop(&mut self) {
        futures::executor::block_on(unsafe { (self.api.stop)(self.pointer) })
            .into_result::<()>()
            .unwrap();
        unsafe { (self.api.destroy)(self.pointer) };
        self.pointer = 0;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.pointer != 0 {
            self.stop();
        }
    }
}
struct Flag {
    value: Mutex<bool>,
    changed: Condvar,
}
impl Flag {
    const fn new() -> Self {
        Self {
            value: Mutex::new(false),
            changed: Condvar::new(),
        }
    }
    fn set(&self) {
        *self.value.lock().unwrap() = true;
        self.changed.notify_all();
    }
    fn get(&self) -> bool {
        *self.value.lock().unwrap()
    }
    fn wait(&self) {
        let (value, _) = self
            .changed
            .wait_timeout_while(
                self.value.lock().unwrap(),
                Duration::from_secs(3),
                |value| !*value,
            )
            .unwrap();
        assert!(
            *value,
            "executor did not reach the expected lifecycle state"
        );
    }
}
struct DropFlag(&'static Flag);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.set();
    }
}

static RETAINED_READY: Flag = Flag::new();
static RETAINED_DROPPED: Flag = Flag::new();
static STOP_TASK_READY: Flag = Flag::new();
static STOP_TASK_DROPPED: Flag = Flag::new();
static RETAINED_ID: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
struct ExecutorProbe {
    retained: Mutex<Option<ManagedTask>>,
}
#[async_trait]
impl Extension for ExecutorProbe {
    fn metadata() -> ExtensionMetadata {
        let mut metadata = ExtensionMetadata::new("test.executor", "Executor regression", "1");
        metadata.services.push(ServiceDescriptor {
            name: "probe.jobs".into(),
            version: 1,
            methods: vec!["join".into()],
        });
        metadata
    }
    async fn start(&self, ctx: Context) -> Result<(), ApiError> {
        tokio::time::sleep(Duration::from_millis(1)).await;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(ApiError::internal)?;
        let (stream, accepted) = tokio::join!(
            tokio::net::TcpStream::connect(listener.local_addr().map_err(ApiError::internal)?),
            listener.accept()
        );
        let mut stream = stream.map_err(ApiError::internal)?;
        let (mut accepted, _) = accepted.map_err(ApiError::internal)?;
        stream
            .write_all(b"executor")
            .await
            .map_err(ApiError::internal)?;
        let mut actual = [0u8; 8];
        accepted
            .read_exact(&mut actual)
            .await
            .map_err(ApiError::internal)?;
        assert_eq!(&actual, b"executor");
        let retained_ctx = ctx.clone();
        let task = ctx.spawn("retained callback context", async move {
            let _guard = DropFlag(&RETAINED_DROPPED);
            tokio::time::sleep(Duration::from_millis(10)).await;
            retained_ctx
                .log(LogLevel::Info, "context survives start completion")
                .await?;
            RETAINED_READY.set();
            futures::future::pending::<Result<(), ApiError>>().await
        })?;
        RETAINED_ID.store(task.id(), Ordering::Release);
        *self.retained.lock().unwrap() = Some(task);
        ctx.spawn("drained on stop", async {
            let _guard = DropFlag(&STOP_TASK_DROPPED);
            STOP_TASK_READY.set();
            futures::future::pending::<Result<(), ApiError>>().await
        })?;
        Ok(())
    }
    async fn on_service(&self, _ctx: Context, _call: ServiceCall) -> Result<Value, ApiError> {
        let task = self.retained.lock().unwrap().as_ref().unwrap().clone();
        task.join().await?;
        Ok(Value::Null)
    }
    async fn stop(&self, ctx: Context) -> Result<(), ApiError> {
        assert!(RETAINED_DROPPED.get());
        assert!(STOP_TASK_DROPPED.get());
        ctx.log(LogLevel::Info, "cleanup calls remain available")
            .await
    }
}

#[test]
fn callbacks_and_background_jobs_share_managed_tokio_and_stop_drains_scope() {
    let mut fixture = Fixture::new::<ExecutorProbe>();
    fixture.start().unwrap();
    RETAINED_READY.wait();
    STOP_TASK_READY.wait();
    let id = RETAINED_ID.load(Ordering::Acquire);
    assert_ne!(id & NATIVE_TASK_BIT, 0);
    let cancel = ServiceCall {
        service: TASK_SERVICE.into(),
        method: "cancel".into(),
        account: None,
        args: to_value(&std::collections::BTreeMap::from([("id", id)])).unwrap(),
        caller: "host".into(),
        caller_kind: atri_core::CallerKind::Runtime,
    };
    fixture.call(cancel).unwrap();
    RETAINED_DROPPED.wait();
    let join = ServiceCall {
        service: "probe.jobs".into(),
        method: "join".into(),
        account: None,
        args: empty_map(),
        caller: "host".into(),
        caller_kind: atri_core::CallerKind::Runtime,
    };
    assert_eq!(fixture.call(join.clone()).unwrap_err().code, "cancelled");
    let mut unknown = join;
    unknown.method = "undeclared".into();
    assert_eq!(
        fixture.call(unknown.clone()).unwrap_err().code,
        "unknown_method"
    );
    unknown.service = "undeclared".into();
    assert_eq!(fixture.call(unknown).unwrap_err().code, "unknown_service");
    fixture.stop();
    let notices = fixture.host.notices.lock().unwrap();
    assert_eq!(
        notices
            .iter()
            .filter(|notice| notice.state == "running")
            .count(),
        2
    );
    assert_eq!(
        notices
            .iter()
            .filter(|notice| notice.state == "cancelled")
            .count(),
        2
    );
}

static CALLBACK_READY: Flag = Flag::new();
static CALLBACK_DROPPED: Flag = Flag::new();
#[derive(Default)]
struct CancelProbe;
#[async_trait]
impl Extension for CancelProbe {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("test.cancel", "Cancellation regression", "1")
    }
    async fn on_event(&self, _ctx: Context, _event: Event) -> Result<EventDisposition, ApiError> {
        let _guard = DropFlag(&CALLBACK_DROPPED);
        CALLBACK_READY.set();
        futures::future::pending().await
    }
}
#[test]
fn dropping_unpolled_ffi_completion_cancels_enqueued_callback() {
    let mut fixture = Fixture::new::<CancelProbe>();
    fixture.start().unwrap();
    let event = Event {
        id: 1,
        source: "host".into(),
        time_ms: 0,
        kind: "test".into(),
        account: None,
        payload: Value::Null,
    };
    let completion = unsafe {
        (fixture.api.event)(
            fixture.pointer,
            OwnedBuffer::from_vec(encode(&event).unwrap()),
        )
    };
    CALLBACK_READY.wait();
    // The callback was queued when the ABI function ran, not when its returned
    // future was polled. Dropping even that unpolled future must cancel it.
    drop(completion);
    CALLBACK_DROPPED.wait();
    fixture.stop();
}

static DEADLINE_DROPPED: Flag = Flag::new();
#[derive(Default)]
struct DeadlineProbe;
#[async_trait]
impl Extension for DeadlineProbe {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("test.deadline", "Deadline regression", "1")
    }
    async fn start(&self, ctx: Context) -> Result<(), ApiError> {
        let task = ctx.spawn_with_timeout("bounded job", Duration::from_millis(10), async {
            let _guard = DropFlag(&DEADLINE_DROPPED);
            futures::future::pending().await
        })?;
        assert_eq!(task.join().await.unwrap_err().code, "deadline_exceeded");
        assert!(DEADLINE_DROPPED.get());
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn managed_deadline_drops_task_and_stop_is_safe_inside_an_async_executor() {
    let mut fixture = Fixture::new::<DeadlineProbe>();
    unsafe { (fixture.api.start)(fixture.pointer) }
        .await
        .into_result::<()>()
        .unwrap();
    let notices = fixture.host.notices.lock().unwrap();
    assert!(notices.iter().any(|notice| notice.state == "timeout"));
    drop(notices);
    fixture.stop();
}

#[derive(Default)]
struct OrdinaryMetadata;
#[async_trait]
impl Extension for OrdinaryMetadata {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("abc", "Ordinary ASCII identity", "1")
    }
}

#[derive(Default)]
struct UuidMetadata;
#[async_trait]
impl Extension for UuidMetadata {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("9f84870e-a135-4d1e-bbb6-934395abfe00", "UUID identity", "1")
    }
}

#[derive(Default)]
struct ArbitraryMetadata;
#[async_trait]
impl Extension for ArbitraryMetadata {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new(
            "\0../Arbitrary ASCII: \x7f",
            "Arbitrary ASCII identity",
            "1",
        )
    }
}

#[derive(Default)]
struct NonAsciiMetadata;
#[async_trait]
impl Extension for NonAsciiMetadata {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("non-ASCII-é", "Invalid non-ASCII identity", "1")
    }
}

#[derive(Default)]
struct EmptyMetadata;
#[async_trait]
impl Extension for EmptyMetadata {
    fn metadata() -> ExtensionMetadata {
        ExtensionMetadata::new("", "Invalid empty identity", "1")
    }
}

#[test]
fn native_metadata_preserves_every_nonempty_ascii_identity() {
    fn metadata_id<E: Extension + Default>() -> String {
        let api = exports::<E>();
        let metadata: ExtensionMetadata = unsafe { (api.metadata)() }.into_result().unwrap();
        assert_eq!(metadata.id, E::metadata().id);
        metadata.id
    }
    assert_eq!(metadata_id::<OrdinaryMetadata>(), "abc");
    assert_eq!(
        metadata_id::<UuidMetadata>(),
        "9f84870e-a135-4d1e-bbb6-934395abfe00",
    );
    assert_eq!(
        metadata_id::<ArbitraryMetadata>(),
        "\0../Arbitrary ASCII: \x7f"
    );
}

#[test]
fn native_metadata_rejects_empty_and_non_ascii_identity() {
    for api in [exports::<EmptyMetadata>(), exports::<NonAsciiMetadata>()] {
        assert_eq!(
            unsafe { (api.metadata)() }
                .into_result::<ExtensionMetadata>()
                .unwrap_err()
                .code,
            "invalid_extension_id",
        );
    }
}
