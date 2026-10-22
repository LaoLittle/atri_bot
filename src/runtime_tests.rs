use super::*;
use crate::control::ControlClient;
use serde::Deserialize;
use tempfile::TempDir;

fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:02x}")).collect()
}
fn response(value: Value) -> Vec<u8> {
    encode(&Ok::<Value, ApiError>(value)).unwrap()
}
fn packed(offset: u64, bytes: &[u8]) -> u64 {
    (offset << 32) | bytes.len() as u64
}
/// A first-class custom core-module extension fixture (no WASI imports).
/// Registration and notifications cross its bounded async host ABI.
fn guest(
    metadata: &ExtensionMetadata,
    disposition: EventDisposition,
    start_instruction: &str,
) -> String {
    guest_program(metadata, disposition, start_instruction, None, &[])
}
fn guest_program(
    metadata: &ExtensionMetadata,
    disposition: EventDisposition,
    start_instruction: &str,
    service_instruction: Option<&str>,
    extra: &[u8],
) -> String {
    let metadata_frame = response(to_value(metadata).unwrap());
    let unit = response(Value::Null);
    let event = response(to_value(&disposition).unwrap());
    let answer = response(Value::Integer(42.into()));
    let marker = encode(
        &Request::new(
            "events.emit",
            PublishEvent {
                kind: "test.marker".into(),
                account: None,
                payload: Value::Text(metadata.id.clone()),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: metadata
                    .services
                    .first()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "test.echo".into()),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let service_instruction = service_instruction
        .map(str::to_owned)
        .unwrap_or_else(|| format!("i64.const {}", packed(10000, &answer)));
    format!(
        r#"(module
      (import "atri_v1" "call" (func $call (param i32 i32) (result i32)))
      (import "atri_v1" "response_drop" (func $drop (param i32)))
      (import "atri_v1" "response_len" (func $response_len (param i32) (result i32)))
      (import "atri_v1" "response_read" (func $response_read (param i32 i32 i32) (result i32)))
      (memory (export "memory") 1 2)
      (data (i32.const 0) "{}")
      (data (i32.const 8192) "{}")
      (data (i32.const 9000) "{}")
      (data (i32.const 10000) "{}")
      (data (i32.const 12000) "{}")
      (data (i32.const 16000) "{}")
      (data (i32.const 20000) "{}")
      (func (export "atri_abi_version") (result i32) i32.const 1)
      (func (export "atri_alloc") (param i32) (result i32) i32.const 32768)
      (func (export "atri_free") (param i32 i32))
      (func (export "atri_invoke") (param $op i32) (param $ptr i32) (param $size i32) (result i64)
        (local $handle i32) (local $len i32)
        local.get $op i32.eqz
        if (result i64) i64.const {}
        else
          local.get $op i32.const 1 i32.eq
          if (result i64)
            {}
            i64.const {}
          else
            local.get $op i32.const 2 i32.eq
            if (result i64)
              i32.const 12000 i32.const {} call $call call $drop
              i64.const {}
            else
              local.get $op i32.const 3 i32.eq
              if (result i64) {}
              else i64.const {} end
            end
          end
        end))"#,
        escaped(&metadata_frame),
        escaped(&unit),
        escaped(&event),
        escaped(&answer),
        escaped(&marker),
        escaped(&registration),
        escaped(extra),
        packed(0, &metadata_frame),
        start_instruction,
        packed(8192, &unit),
        marker.len(),
        packed(9000, &event),
        service_instruction,
        packed(8192, &unit),
    )
}
fn echo_guest(metadata: &ExtensionMetadata, start: &str) -> String {
    // Wrap the real incoming CBOR ServiceCall in the ABI's successful result.
    let mut prefix = response(Value::Null);
    assert_eq!(prefix.pop(), Some(0xf6));
    let instruction = format!(
        "i32.const 40000 i32.const 20000 i32.const {} memory.copy i32.const {} local.get $ptr local.get $size memory.copy i64.const 40000 i64.const 32 i64.shl local.get $size i32.const {} i32.add i64.extend_i32_u i64.or",
        prefix.len(),
        40000 + prefix.len(),
        prefix.len()
    );
    guest_program(
        metadata,
        EventDisposition::Continue,
        start,
        Some(&instruction),
        &prefix,
    )
}
fn service_proxy(length: usize) -> String {
    format!(
        "i32.const 20000 i32.const {length} call $call local.set $handle local.get $handle call $response_len local.set $len local.get $handle i32.const 40000 local.get $len call $response_read drop local.get $handle call $drop i64.const 40000 i64.const 32 i64.shl local.get $len i64.extend_i32_u i64.or"
    )
}
fn metadata(id: &str) -> ExtensionMetadata {
    let mut metadata = ExtensionMetadata::new(id, id, "1.0.0");
    metadata.capabilities.extend([
        "events.emit".into(),
        "services.register".into(),
        "tasks.spawn".into(),
    ]);
    metadata
}
async fn deploy(root: &Path, name: &str, source: &str) {
    let directory = root.join("extensions").join("nested").join(name);
    tokio::fs::create_dir_all(&directory).await.unwrap();
    tokio::fs::write(directory.join("extension.wasm"), source)
        .await
        .unwrap();
}
async fn settled(runtime: &Runtime) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = runtime.snapshot().await;
            if snapshot
                .extensions
                .iter()
                .all(|p| !matches!(p.state, ExtensionState::Starting))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
async fn markers(receiver: &mut broadcast::Receiver<RuntimeNotice>, count: usize) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        while output.len() < count {
            if let RuntimeNotice::Event(event) = receiver.recv().await.unwrap() {
                if matches!(&event.kind, EventKind::Custom(name) if name == "test.marker") {
                    assert_eq!(event.payload, Value::Text(event.source.clone()));
                    output.push(event.source);
                }
            }
        }
        output
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn actual_guest_priority_interception_observers_and_online_broadcast() {
    let root = TempDir::new().unwrap();
    for (id, priority, observer, disposition) in [
        ("test.high", 100, false, EventDisposition::Stop),
        ("test.low", 0, false, EventDisposition::Continue),
        ("test.observer", -100, true, EventDisposition::Stop),
    ] {
        let mut metadata = metadata(id);
        metadata.subscriptions = vec![
            Subscription {
                kind: EventKind::MessageReceived,
                priority,
                observer,
            },
            Subscription {
                kind: EventKind::BotOnline,
                priority,
                observer,
            },
        ];
        deploy(root.path(), id, &guest(&metadata, disposition, "")).await;
    }
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    assert_eq!(runtime.snapshot().await.extensions.len(), 3);
    let mut notices = runtime.subscribe();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    runtime
        .inner
        .publish(
            "test.driver",
            PublishEvent {
                kind: EventKind::MessageReceived,
                account: None,
                payload: Value::Null,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        markers(&mut notices, 2).await,
        vec!["test.high", "test.observer"]
    );
    runtime
        .inner
        .publish(
            "test.driver",
            PublishEvent {
                kind: EventKind::BotOnline,
                account: Some("account".into()),
                payload: to_value(&BotOnline {
                    bot: UserIdentity {
                        account: "account".into(),
                        id: "bot".into(),
                        display_name: None,
                    },
                })
                .unwrap(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        markers(&mut notices, 3).await,
        vec!["test.high", "test.low", "test.observer"]
    );
    runtime.shutdown().await.unwrap();
    assert!(!runtime.snapshot().await.running);
}
#[tokio::test]
async fn actual_guest_service_capability_control_authentication_and_lifecycle() {
    let root = TempDir::new().unwrap();
    let mut metadata = metadata("test.service");
    metadata.services.push(ServiceDescriptor {
        name: "test.echo".into(),
        version: 1,
        methods: vec!["value".into()],
    });
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: "test.echo".into(),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let start = format!(
        "i32.const 16000 i32.const {} call $call call $drop",
        registration.len()
    );
    deploy(
        root.path(),
        "service",
        &guest(&metadata, EventDisposition::Continue, &start),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let client = ControlClient::connect(root.path()).await.unwrap().unwrap();
    let call = ServiceCall {
        service: "test.echo".into(),
        method: "value".into(),
        account: None,
        args: Value::Null,
        caller: "spoofed".into(),
        caller_kind: CallerKind::Unknown,
    };
    let result = client
        .request(Request::new("services.call", &call).unwrap())
        .await
        .unwrap();
    assert_eq!(result, Value::Integer(42.into()));
    let denied = runtime
        .inner
        .host(
            &metadata.id,
            CancellationToken::new(),
            Request::new(
                "output.table",
                OutputTable {
                    title: "denied".into(),
                    columns: vec![],
                    rows: vec![],
                },
            )
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(denied.code, "capability_denied");
    let endpoint = crate::storage::read_control_metadata(root.path())
        .await
        .unwrap()
        .unwrap();
    #[derive(Deserialize)]
    struct Endpoint {
        address: String,
        session: String,
    }
    let endpoint: Endpoint = from_value(&endpoint).unwrap();
    let unauthorized = reqwest::Client::new()
        .post(format!(
            "http://{}/v1/{}",
            endpoint.address, endpoint.session
        ))
        .body(encode(&Request::new("runtime.status", Value::Null).unwrap()).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    client
        .request(Request::new("extensions.disable", serde_json::json!({"id":metadata.id})).unwrap())
        .await
        .unwrap();
    let missing = client
        .request(Request::new("services.call", &call).unwrap())
        .await
        .unwrap_err();
    assert_eq!(missing.code, "service_not_found");
    client
        .request(Request::new("extensions.enable", serde_json::json!({"id":metadata.id})).unwrap())
        .await
        .unwrap();
    settled(&runtime).await;
    assert_eq!(
        client
            .request(Request::new("services.call", &call).unwrap())
            .await
            .unwrap(),
        Value::Integer(42.into())
    );
    runtime.shutdown().await.unwrap();
    assert!(ControlClient::connect(root.path()).await.unwrap().is_none());
}
#[tokio::test]
async fn actual_guest_service_origin_overrides_forged_wasm_and_authenticated_control_payloads() {
    let root = TempDir::new().unwrap();
    let mut provider = metadata("test.origin.provider");
    provider.services.push(ServiceDescriptor {
        name: "test.origin".into(),
        version: 1,
        methods: vec!["inspect".into()],
    });
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: "test.origin".into(),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let start = format!(
        "i32.const 16000 i32.const {} call $call call $drop",
        registration.len()
    );
    deploy(root.path(), "provider", &echo_guest(&provider, &start)).await;
    let mut proxy = metadata("test.origin.wasm");
    proxy.capabilities.push("services.call".into());
    proxy.services.push(ServiceDescriptor {
        name: "test.proxy".into(),
        version: 1,
        methods: vec!["inspect".into()],
    });
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: "test.proxy".into(),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let start = format!(
        "i32.const 16000 i32.const {} call $call call $drop",
        registration.len()
    );
    let forged = ServiceCall {
        service: "test.origin".into(),
        method: "inspect".into(),
        account: None,
        args: Value::Text("unchanged".into()),
        caller: "atri.control".into(),
        caller_kind: CallerKind::Runtime,
    };
    let request = encode(&Request::new("services.call", &forged).unwrap()).unwrap();
    deploy(
        root.path(),
        "proxy",
        &guest_program(
            &proxy,
            EventDisposition::Continue,
            &start,
            Some(&service_proxy(request.len())),
            &request,
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let client = ControlClient::connect(root.path()).await.unwrap().unwrap();
    let mut call = forged.clone();
    call.service = "test.proxy".into();
    let observed: ServiceCall = from_value(
        &client
            .request(Request::new("services.call", call).unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(observed.caller, proxy.id);
    assert_eq!(observed.caller_kind, CallerKind::Wasm);
    assert_eq!(observed.args, forged.args);
    let mut control = forged;
    control.caller = proxy.id;
    control.caller_kind = CallerKind::Wasm;
    let observed: ServiceCall = from_value(
        &client
            .request(Request::new("services.call", &control).unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(observed.caller, "atri.control");
    assert_eq!(observed.caller_kind, CallerKind::Runtime);
    #[derive(Deserialize)]
    struct Endpoint {
        address: String,
        session: String,
    }
    let endpoint: Endpoint = from_value(
        &crate::storage::read_control_metadata(root.path())
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let unauthorized = reqwest::Client::new()
        .post(format!(
            "http://{}/v1/{}",
            endpoint.address, endpoint.session
        ))
        .body(encode(&Request::new("services.call", control).unwrap()).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_rescan_discovery_and_duplicates_publish_nothing_and_retry_all_guests() {
    for duplicate in [false, true] {
        let root = TempDir::new().unwrap();
        let runtime = Runtime::open(root.path().into()).await.unwrap();
        runtime.start().await.unwrap();
        for id in ["test.a", "test.b"] {
            let mut metadata = metadata(id);
            metadata.services.push(ServiceDescriptor {
                name: id.into(),
                version: 1,
                methods: vec!["value".into()],
            });
            let registration = encode(
                &Request::new(
                    "services.register",
                    ServiceBinding {
                        service: id.into(),
                        account: None,
                    },
                )
                .unwrap(),
            )
            .unwrap();
            deploy(
                root.path(),
                id,
                &guest(
                    &metadata,
                    EventDisposition::Continue,
                    &format!(
                        "i32.const 16000 i32.const {} call $call call $drop",
                        registration.len()
                    ),
                ),
            )
            .await;
        }
        let invalid = if duplicate {
            guest(&metadata("test.a"), EventDisposition::Continue, "")
        } else {
            "not a Wasm artifact".into()
        };
        deploy(root.path(), "z-invalid", &invalid).await;
        let error = runtime
            .control(Request::new("extensions.rescan", Value::Null).unwrap())
            .await
            .unwrap_err();
        if duplicate {
            assert_eq!(error.code, "duplicate_extension");
        }
        assert!(runtime.snapshot().await.extensions.is_empty());
        assert!(runtime.inner.services.read().await.is_empty());
        assert!(runtime.inner.tasks.list(None).is_empty());
        deploy(
            root.path(),
            "z-invalid",
            &guest(&metadata("test.c"), EventDisposition::Continue, ""),
        )
        .await;
        runtime
            .control(Request::new("extensions.rescan", Value::Null).unwrap())
            .await
            .unwrap();
        settled(&runtime).await;
        let snapshot = runtime.snapshot().await;
        assert_eq!(snapshot.extensions.len(), 3);
        assert!(
            snapshot
                .extensions
                .iter()
                .all(|plugin| matches!(plugin.state, ExtensionState::Running))
        );
        for id in ["test.a", "test.b"] {
            let call = ServiceCall {
                service: id.into(),
                method: "value".into(),
                account: None,
                args: Value::Null,
                caller: "untrusted".into(),
                caller_kind: CallerKind::Unknown,
            };
            assert_eq!(
                runtime
                    .control(Request::new("services.call", call).unwrap())
                    .await
                    .unwrap(),
                Value::Integer(42.into())
            );
        }
        assert!(runtime.inner.tasks.list(None).is_empty());
        runtime.shutdown().await.unwrap();
        assert!(runtime.inner.services.read().await.is_empty());
    }
}

#[tokio::test]
async fn guest_instantiation_cannot_register_services_before_host_context_is_attached() {
    let root = TempDir::new().unwrap();
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    let mut metadata = metadata("test.prepare");
    metadata.services.push(ServiceDescriptor {
        name: "test.prepare".into(),
        version: 1,
        methods: vec!["value".into()],
    });
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: "test.prepare".into(),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    // Both metadata discovery and live instantiation must deny this early call.
    // If either gives it live host authority, ABI validation deliberately fails.
    let version = format!(
        "(func (export \"atri_abi_version\") (result i32) (local $handle i32) (local $len i32) i32.const 16000 i32.const {} call $call local.tee $handle call $response_len local.set $len local.get $handle call $drop local.get $len i32.const {} i32.eq if (result i32) i32.const 2 else i32.const 1 end)",
        registration.len(),
        response(Value::Null).len()
    );
    let source = guest(&metadata, EventDisposition::Continue, "").replace(
        "(func (export \"atri_abi_version\") (result i32) i32.const 1)",
        &version,
    );
    deploy(root.path(), "early-call", &source).await;
    runtime
        .control(Request::new("extensions.rescan", Value::Null).unwrap())
        .await
        .unwrap();
    settled(&runtime).await;
    assert!(matches!(
        runtime.snapshot().await.extensions[0].state,
        ExtensionState::Running
    ));
    assert!(runtime.inner.services.read().await.is_empty());
    runtime.shutdown().await.unwrap();
}
#[tokio::test]
async fn guest_fuel_exhaustion_is_reported_and_does_not_claim_running() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("test.fuel");
    deploy(
        root.path(),
        "fuel",
        &guest(
            &metadata,
            EventDisposition::Continue,
            "(loop $forever br $forever)",
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let snapshot = runtime.snapshot().await;
    assert!(matches!(
        snapshot.extensions[0].state,
        ExtensionState::Failed
    ));
    assert!(
        snapshot.extensions[0]
            .error
            .as_ref()
            .unwrap()
            .contains("fuel")
    );
    runtime.shutdown().await.unwrap();
}
#[tokio::test]
async fn host_timer_cancellation_removes_task_and_is_not_a_fake_completion() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("test.timer");
    deploy(
        root.path(),
        "timer",
        &guest(&metadata, EventDisposition::Continue, ""),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let scope = CancellationToken::new();
    let task = tokio::spawn(
        runtime.inner.host(
            &metadata.id,
            scope.clone(),
            Request::new(
                "tasks.sleep",
                SleepRequest {
                    milliseconds: 60_000,
                },
            )
            .unwrap(),
        ),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while runtime.inner.tasks.list(None).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let id = runtime.inner.tasks.list(None)[0].id;
    runtime
        .control(Request::new("tasks.cancel", TaskId { id }).unwrap())
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    assert!(runtime.inner.tasks.list(None).is_empty());
    runtime.shutdown().await.unwrap();
}

async fn external_probe(variable: &str, id: &str, event_kind: EventKind) {
    let artifact = PathBuf::from(
        std::env::var(variable)
            .expect("build independent probe and set its absolute artifact path"),
    );
    assert!(artifact.is_absolute());
    let root = TempDir::new().unwrap();
    let directory = root.path().join("extensions").join(id);
    tokio::fs::create_dir_all(&directory).await.unwrap();
    tokio::fs::copy(&artifact, directory.join(artifact.file_name().unwrap()))
        .await
        .unwrap();
    if variable == "ATRI_NATIVE_PROBE" {
        let mut provider = metadata("test.native.origin");
        provider.services.push(ServiceDescriptor {
            name: "test.native.origin".into(),
            version: 1,
            methods: vec!["inspect".into()],
        });
        let registration = encode(
            &Request::new(
                "services.register",
                ServiceBinding {
                    service: "test.native.origin".into(),
                    account: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        deploy(
            root.path(),
            "origin-provider",
            &echo_guest(
                &provider,
                &format!(
                    "i32.const 16000 i32.const {} call $call call $drop",
                    registration.len()
                ),
            ),
        )
        .await;
    }
    for expected in [1u64, 2] {
        let runtime = Runtime::open(root.path().into()).await.unwrap();
        let mut notices = runtime.subscribe();
        runtime.start().await.unwrap();
        let event = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let RuntimeNotice::Event(event) = notices.recv().await.unwrap() {
                    if event.kind == event_kind {
                        break event;
                    }
                }
            }
        })
        .await
        .unwrap();
        #[derive(Deserialize)]
        struct Counter {
            runs: u64,
        }
        assert_eq!(event.source, id);
        assert_eq!(event.decode::<Counter>().unwrap().runs, expected);
        let config: ConfigSnapshot = from_value(
            &runtime
                .control(Request::new("config.get", serde_json::json!({"id":id})).unwrap())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(from_value::<Counter>(&config.value).unwrap().runs, expected);
        if variable == "ATRI_NATIVE_PROBE" {
            settled(&runtime).await;
            let scope = runtime
                .inner
                .plugin(id)
                .await
                .unwrap()
                .scope
                .lock()
                .await
                .clone();
            let forged = ServiceCall {
                service: "test.native.origin".into(),
                method: "inspect".into(),
                account: None,
                args: Value::Null,
                caller: "atri.control".into(),
                caller_kind: CallerKind::Runtime,
            };
            let observed: ServiceCall = from_value(
                &runtime
                    .inner
                    .host(id, scope, Request::new("services.call", forged).unwrap())
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(observed.caller, id);
            assert_eq!(observed.caller_kind, CallerKind::Native);
        }
        runtime.shutdown().await.unwrap();
        assert!(runtime.inner.tasks.list(None).is_empty());
    }
}
#[tokio::test]
#[ignore = "requires independently built native_probe; set ATRI_NATIVE_PROBE to its absolute library path"]
async fn separately_built_native_sdk_executes_managed_tokio_io_and_persists_config() {
    external_probe(
        "ATRI_NATIVE_PROBE",
        "io.atri.probe.native",
        EventKind::Custom("io.atri.probe.native.completed".into()),
    )
    .await;
}
#[tokio::test]
#[ignore = "requires independently built wasm_probe; set ATRI_WASM_PROBE to its absolute .wasm path"]
async fn separately_built_rust_wasm_sdk_executes_async_host_calls_and_persists_config() {
    external_probe(
        "ATRI_WASM_PROBE",
        "io.atri.probe.wasm",
        EventKind::Custom("io.atri.probe.wasm.completed".into()),
    )
    .await;
}

#[tokio::test]
async fn cross_store_service_dependency_cycle_is_rejected_without_deadlock() {
    let root = TempDir::new().unwrap();
    for (id, service, target) in [
        ("test.a", "test.a", "test.b"),
        ("test.b", "test.b", "test.a"),
    ] {
        let mut metadata = metadata(id);
        metadata.capabilities.push("services.call".into());
        metadata.services.push(ServiceDescriptor {
            name: service.into(),
            version: 1,
            methods: vec!["proxy".into()],
        });
        let registration = encode(
            &Request::new(
                "services.register",
                ServiceBinding {
                    service: service.into(),
                    account: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let start = format!(
            "i32.const 16000 i32.const {} call $call call $drop",
            registration.len()
        );
        let call = encode(
            &Request::new(
                "services.call",
                ServiceCall {
                    service: target.into(),
                    method: "proxy".into(),
                    account: None,
                    args: Value::Null,
                    caller: "untrusted".into(),
                    caller_kind: CallerKind::Runtime,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let proxy = service_proxy(call.len());
        deploy(
            root.path(),
            id,
            &guest_program(
                &metadata,
                EventDisposition::Continue,
                &start,
                Some(&proxy),
                &call,
            ),
        )
        .await;
    }
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let call = ServiceCall {
        service: "test.a".into(),
        method: "proxy".into(),
        account: None,
        args: Value::Null,
        caller: "untrusted".into(),
        caller_kind: CallerKind::Unknown,
    };
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        runtime.control(Request::new("services.call", call).unwrap()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code, "service_cycle");
    assert!(runtime.inner.service_graph.0.lock().unwrap().is_empty());
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn extension_identity_can_equal_the_control_callers_display_name() {
    let root = TempDir::new().unwrap();
    let mut metadata = metadata("atri.control");
    metadata.services.push(ServiceDescriptor {
        name: "test.identity".into(),
        version: 1,
        methods: vec!["value".into()],
    });
    let registration = encode(
        &Request::new(
            "services.register",
            ServiceBinding {
                service: "test.identity".into(),
                account: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    deploy(
        root.path(),
        "identity",
        &guest_program(
            &metadata,
            EventDisposition::Continue,
            &format!(
                "i32.const 16000 i32.const {} call $call call $drop",
                registration.len()
            ),
            None,
            &registration,
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let response = runtime
        .control(
            Request::new(
                "services.call",
                ServiceCall {
                    service: "test.identity".into(),
                    method: "value".into(),
                    account: None,
                    args: Value::Null,
                    caller: String::new(),
                    caller_kind: CallerKind::Unknown,
                },
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response, Value::Integer(42.into()));
    runtime.shutdown().await.unwrap();
}
#[tokio::test]
async fn guest_output_allocation_budget_returns_explicit_error() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("test.output");
    deploy(
        root.path(),
        "output",
        &guest(&metadata, EventDisposition::Continue, ""),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let error = runtime
        .inner
        .host(
            &metadata.id,
            CancellationToken::new(),
            Request::new(
                "log",
                LogRecord {
                    level: LogLevel::Info,
                    message: "x".repeat(NOTICE_BYTES + 1),
                    fields: BTreeMap::new(),
                },
            )
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "output_too_large");
    runtime.shutdown().await.unwrap();
}
#[tokio::test]
async fn startup_does_not_wait_for_guest_host_timer_and_shutdown_cancels_it() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("test.startup");
    let request = encode(
        &Request::new(
            "tasks.sleep",
            SleepRequest {
                milliseconds: 60_000,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let start = format!(
        "i32.const 20000 i32.const {} call $call call $drop",
        request.len()
    );
    deploy(
        root.path(),
        "startup",
        &guest_program(
            &metadata,
            EventDisposition::Continue,
            &start,
            None,
            &request,
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), runtime.start())
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while runtime.inner.tasks.list(None).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        runtime.snapshot().await.extensions[0].state,
        ExtensionState::Starting
    ));
    runtime.shutdown().await.unwrap();
    assert!(runtime.inner.tasks.list(None).is_empty());
}
#[tokio::test]
async fn retained_guest_response_bytes_are_bounded_independently_of_handle_count() {
    let root = TempDir::new().unwrap();
    let mut metadata = metadata("test.responses");
    metadata.configuration.format = "cbor".into();
    let request = encode(
        &Request::new(
            "config.load",
            ConfigLoad {
                defaults: None,
                format: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let start = format!(
        "i32.const 20000 i32.const {} call $call drop ",
        request.len()
    )
    .repeat(6);
    deploy(
        root.path(),
        "responses",
        &guest_program(
            &metadata,
            EventDisposition::Continue,
            &start,
            None,
            &request,
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime
        .inner
        .storage
        .config_store(
            &metadata.id,
            &metadata.configuration,
            ConfigStore {
                value: Value::Text("x".repeat(12 * 1024 * 1024)),
                expected_revision: None,
                format: Some("cbor".into()),
            },
        )
        .await
        .unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let snapshot = runtime.snapshot().await;
    assert!(matches!(
        snapshot.extensions[0].state,
        ExtensionState::Failed
    ));
    assert!(
        snapshot.extensions[0]
            .error
            .as_deref()
            .unwrap()
            .contains("response handle quota")
    );
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn guest_memory_range_is_validated_before_host_request_decoding() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("test.memory");
    deploy(
        root.path(),
        "memory",
        &guest(
            &metadata,
            EventDisposition::Continue,
            "i32.const 65530 i32.const 100 call $call drop",
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let snapshot = runtime.snapshot().await;
    assert!(matches!(
        snapshot.extensions[0].state,
        ExtensionState::Failed
    ));
    assert!(
        snapshot.extensions[0]
            .error
            .as_deref()
            .unwrap()
            .contains("guest memory range out of bounds")
    );
    assert!(runtime.inner.services.read().await.is_empty());
    runtime.shutdown().await.unwrap();
}

/// A real component fixture with canonical-ABI lifted exports. These small
/// callbacks complete synchronously; the independent Rust P3 probe exercises
/// async lowering, host services, and WASI clock suspension.
fn component_guest(metadata: &ExtensionMetadata, start: &str, event: &str, stop: &str) -> String {
    let metadata = response(to_value(metadata).unwrap());
    let unit = response(Value::Null);
    let disposition = response(to_value(&EventDisposition::Continue).unwrap());
    let mut records = Vec::new();
    for (ptr, len) in [
        (4096_u32, metadata.len() as u32),
        (8192, unit.len() as u32),
        (9000, disposition.len() as u32),
    ] {
        records.extend_from_slice(&ptr.to_le_bytes());
        records.extend_from_slice(&len.to_le_bytes());
    }
    format!(
        r#"(component
      (core module $m
        (memory (export "memory") 1 2)
        (data (i32.const 0) "{records}")
        (data (i32.const 4096) "{metadata}")
        (data (i32.const 8192) "{unit}")
        (data (i32.const 9000) "{disposition}")
        (func (export "realloc") (param i32 i32 i32 i32) (result i32) i32.const 32768)
        (func (export "describe") (result i32) i32.const 0)
        (func (export "start") (result i32) {start} i32.const 8)
        (func (export "on-event") (param i32 i32) (result i32) {event} i32.const 16)
        (func (export "on-service") (param i32 i32) (result i32) i32.const 8)
        (func (export "stop") (result i32) {stop} i32.const 8))
      (core instance $i (instantiate $m))
      (alias core export $i "memory" (core memory $memory))
      (alias core export $i "realloc" (core func $realloc))
      (alias core export $i "describe" (core func $raw-describe))
      (alias core export $i "start" (core func $raw-start))
      (alias core export $i "on-event" (core func $raw-event))
      (alias core export $i "on-service" (core func $raw-service))
      (alias core export $i "stop" (core func $raw-stop))
      (func $describe (result (list u8)) (canon lift (core func $raw-describe) (memory $memory)))
      (func $start (result (list u8)) (canon lift (core func $raw-start) (memory $memory)))
      (func $event (param "event" (list u8)) (result (list u8))
        (canon lift (core func $raw-event) (memory $memory) (realloc $realloc)))
      (func $service (param "call" (list u8)) (result (list u8))
        (canon lift (core func $raw-service) (memory $memory) (realloc $realloc)))
      (func $stop (result (list u8)) (canon lift (core func $raw-stop) (memory $memory)))
      (instance $plugin
        (export "describe" (func $describe))
        (export "start" (func $start))
        (export "on-event" (func $event))
        (export "on-service" (func $service))
        (export "stop" (func $stop)))
      (export "atri:extension/plugin@1.0.0" (instance $plugin)))"#,
        records = escaped(&records),
        metadata = escaped(&metadata),
        unit = escaped(&unit),
        disposition = escaped(&disposition),
    )
}

#[tokio::test]
async fn component_event_trap_fails_instance_and_shutdown_never_reenters_it() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("component trap");
    // Stop itself traps: successful shutdown proves a failed store is disposed,
    // rather than calling the stop hook of an inconsistent component.
    deploy(
        root.path(),
        "component",
        &component_guest(&metadata, "", "unreachable", "unreachable"),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let plugin = runtime.inner.plugin(&metadata.id).await.unwrap();
    let error = runtime
        .inner
        .call(
            plugin.clone(),
            wasm_abi::EVENT,
            encode(&Event {
                id: 1,
                time_ms: 0,
                kind: "test.trap".into(),
                source: "test.driver".into(),
                account: None,
                payload: Value::Null,
            })
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "wasm_execution");
    assert!(matches!(
        plugin.state.lock().await.0,
        ExtensionState::Failed
    ));
    assert_eq!(
        runtime
            .inner
            .call(plugin, wasm_abi::EVENT, Vec::new())
            .await
            .unwrap_err()
            .code,
        "extension_unavailable"
    );
    tokio::time::timeout(Duration::from_secs(5), runtime.shutdown())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn component_fuel_exhaustion_and_stop_traps_are_disposed_deterministically() {
    for start in ["", "(loop $forever br $forever)"] {
        let root = TempDir::new().unwrap();
        let metadata = metadata("component fuel");
        deploy(
            root.path(),
            "component",
            &component_guest(&metadata, start, "", "unreachable"),
        )
        .await;
        let runtime = Runtime::open(root.path().into()).await.unwrap();
        runtime.start().await.unwrap();
        settled(&runtime).await;
        let state = runtime.snapshot().await.extensions[0].state.clone();
        if start.is_empty() {
            assert!(matches!(state, ExtensionState::Running));
        } else {
            assert!(matches!(state, ExtensionState::Failed));
        }
        tokio::time::timeout(Duration::from_secs(5), runtime.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn component_and_core_discover_identity_only_from_metadata() {
    let root = TempDir::new().unwrap();
    for id in ["../ASCII identity:/\\\\\0", "", "非ASCII"] {
        let metadata = metadata(id);
        for source in [
            component_guest(&metadata, "", "", ""),
            guest(&metadata, EventDisposition::Continue, ""),
        ] {
            let path = root.path().join("metadata.wasm");
            tokio::fs::write(&path, source).await.unwrap();
            match WasmModule::load(&path).await {
                Ok((_, discovered)) => {
                    assert!(!id.is_empty() && id.is_ascii());
                    assert_eq!(discovered.id, id);
                }
                Err(error) => {
                    assert!(id.is_empty() || !id.is_ascii(), "{error}");
                    assert_eq!(error.code, "invalid_extension_id");
                }
            }
        }
    }
}

#[tokio::test]
async fn wasi_preview_one_and_preview_two_are_not_linked() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("unsupported WASI");
    for source in [
        guest(&metadata, EventDisposition::Continue, "").replacen(
            "(module",
            r#"(module (import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))"#,
            1,
        ),
        component_guest(&metadata, "", "", "").replacen(
            "(component",
            r#"(component (import "wasi:clocks/monotonic-clock@0.2.0" (instance (export "now" (func (result u64)))))"#,
            1,
        ),
    ] {
        let path = root.path().join("unsupported.wasm");
        tokio::fs::write(&path, source).await.unwrap();
        assert!(
            WasmModule::load(&path).await.is_err(),
            "unsupported WASI interface was linked"
        );
    }
}

#[tokio::test]
async fn component_typed_callbacks_and_graceful_restart_complete() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("component callbacks");
    deploy(
        root.path(),
        "component",
        &component_guest(&metadata, "", "", ""),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    for iteration in 0..2 {
        settled(&runtime).await;
        let plugin = runtime.inner.plugin(&metadata.id).await.unwrap();
        let disposition = runtime
            .inner
            .call(
                plugin.clone(),
                wasm_abi::EVENT,
                encode(&Event {
                    id: 1,
                    time_ms: 0,
                    kind: "test.callback".into(),
                    source: "test.driver".into(),
                    account: None,
                    payload: Value::Null,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            from_value::<EventDisposition>(&disposition).unwrap(),
            EventDisposition::Continue
        );
        assert_eq!(
            runtime
                .inner
                .call(
                    plugin.clone(),
                    wasm_abi::SERVICE,
                    encode(&ServiceCall {
                        service: "test".into(),
                        method: "callback".into(),
                        account: None,
                        args: Value::Null,
                        caller: "test.driver".into(),
                        caller_kind: CallerKind::Runtime,
                    })
                    .unwrap()
                )
                .await
                .unwrap(),
            Value::Null
        );
        runtime.inner.stop(&plugin).await.unwrap();
        assert!(matches!(
            plugin.state.lock().await.0,
            ExtensionState::Disabled
        ));
        if iteration == 0 {
            runtime.inner.prepare(&plugin).await.unwrap();
            runtime.inner.schedule(plugin).await;
        }
    }
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_uses_flat_role_directories_not_metadata_identity_paths() {
    for (role, directory) in [
        (ExtensionRole::Adapter, "adapter"),
        (ExtensionRole::Service, "service"),
    ] {
        let root = TempDir::new().unwrap();
        let artifacts = TempDir::new().unwrap();
        let mut metadata = metadata("../arbitrary ASCII:/\\C:\0\r\n");
        metadata.role = role;
        let source = artifacts.path().join("artifact.wasm");
        let program = guest(&metadata, EventDisposition::Continue, "");
        tokio::fs::write(&source, &program).await.unwrap();
        let runtime = Runtime::open(root.path().into()).await.unwrap();
        let info: ExtensionInfo =
            from_value(&runtime.inner.install(source.clone()).await.unwrap()).unwrap();
        let destination = root
            .path()
            .join("extensions")
            .join(directory)
            .join("artifact.wasm");
        assert_eq!(info.metadata.id, metadata.id);
        assert_eq!(info.metadata.role, role);
        assert_eq!(PathBuf::from(info.path), destination);
        assert!(matches!(info.state, ExtensionState::Disabled));
        assert_eq!(
            tokio::fs::read(&destination).await.unwrap(),
            program.as_bytes()
        );
        assert!(source.is_file());
        runtime.shutdown().await.unwrap();
        drop(runtime);

        // A fresh discovery uses the metadata ID unchanged, regardless of the
        // artifact's role directory or filesystem-safe filename.
        let reopened = Runtime::open(root.path().into()).await.unwrap();
        assert_eq!(
            reopened.snapshot().await.extensions[0].metadata.id,
            metadata.id
        );
        reopened.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn install_rejects_existing_destination_without_replacing_or_registering() {
    let root = TempDir::new().unwrap();
    let artifacts = TempDir::new().unwrap();
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    let metadata = metadata("different identity");
    let source = artifacts.path().join("collision.wasm");
    tokio::fs::write(&source, guest(&metadata, EventDisposition::Continue, ""))
        .await
        .unwrap();
    let directory = root.path().join("extensions").join("service");
    tokio::fs::create_dir_all(&directory).await.unwrap();
    let destination = directory.join("collision.wasm");
    let existing = b"an existing artifact must never be overwritten";
    tokio::fs::write(&destination, existing).await.unwrap();

    let error = runtime.inner.install(source).await.unwrap_err();
    assert_eq!(error.code, "install_destination_exists");
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), existing);
    assert!(runtime.snapshot().await.extensions.is_empty());
    let mut entries = tokio::fs::read_dir(&directory).await.unwrap();
    assert_eq!(
        entries.next_entry().await.unwrap().unwrap().path(),
        destination
    );
    assert!(entries.next_entry().await.unwrap().is_none());
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn core_service_trap_discards_instance_and_shutdown_does_not_reenter_it() {
    let root = TempDir::new().unwrap();
    let metadata = metadata("core trap");
    deploy(
        root.path(),
        "core",
        &guest_program(
            &metadata,
            EventDisposition::Continue,
            "",
            Some("unreachable"),
            &[],
        ),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let plugin = runtime.inner.plugin(&metadata.id).await.unwrap();
    let error = runtime
        .inner
        .call(plugin.clone(), wasm_abi::SERVICE, Vec::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, "wasm_execution");
    assert!(matches!(
        plugin.state.lock().await.0,
        ExtensionState::Failed
    ));
    assert!(matches!(
        plugin.instance.read().await.as_ref(),
        Some(Instance::Wasm(wasm)) if wasm.is_failed()
    ));
    assert_eq!(
        runtime
            .inner
            .call(plugin, wasm_abi::SERVICE, Vec::new())
            .await
            .unwrap_err()
            .code,
        "extension_unavailable"
    );
    tokio::time::timeout(Duration::from_secs(5), runtime.shutdown())
        .await
        .unwrap()
        .unwrap();
}

async fn lifecycle_publish(
    runtime: &Runtime,
    source: &str,
    publish: PublishEvent,
) -> Result<(), ApiError> {
    let plugin = runtime.inner.plugin(source).await?;
    let token = plugin.scope.lock().await.clone();
    runtime
        .inner
        .host(source, token, Request::new("events.emit", publish)?)
        .await?;
    Ok(())
}
fn online_publication(account: &str, id: &str) -> PublishEvent {
    PublishEvent {
        kind: EventKind::BotOnline,
        account: Some(account.into()),
        payload: to_value(&BotOnline {
            bot: UserIdentity {
                account: account.into(),
                id: id.into(),
                display_name: Some("Bot".into()),
            },
        })
        .unwrap(),
    }
}
async fn bot_snapshot(runtime: &Runtime) -> Vec<BotState> {
    from_value(
        &runtime
            .control(Request::new("bots.list", ()).unwrap())
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn bot_snapshots_are_source_owned_capability_gated_and_visible_to_late_extensions() {
    let root = TempDir::new().unwrap();
    for id in ["source.a", "source.b"] {
        deploy(
            root.path(),
            id,
            &guest(&metadata(id), EventDisposition::Continue, ""),
        )
        .await;
    }
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    lifecycle_publish(&runtime, "source.a", online_publication("shared", "bot.a"))
        .await
        .unwrap();
    lifecycle_publish(&runtime, "source.b", online_publication("shared", "bot.b"))
        .await
        .unwrap();
    let source_a = runtime.inner.plugin("source.a").await.unwrap();
    let old_token = source_a.scope.lock().await.clone();
    let denied = runtime
        .inner
        .host(
            "source.a",
            old_token.clone(),
            Request::new("bots.list", ()).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(denied.code, "capability_denied");

    // This subscriber did not exist when either lifecycle event was published.
    let mut reader = metadata("late.reader");
    reader.capabilities.push("bots.read".into());
    deploy(
        root.path(),
        "late.reader",
        &guest(&reader, EventDisposition::Continue, ""),
    )
    .await;
    runtime.inner.rescan().await.unwrap();
    settled(&runtime).await;
    let reader_plugin = runtime.inner.plugin("late.reader").await.unwrap();
    let reader_token = reader_plugin.scope.lock().await.clone();
    let snapshot: Vec<BotState> = from_value(
        &runtime
            .inner
            .host(
                "late.reader",
                reader_token,
                Request::new("bots.list", ()).unwrap(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot, bot_snapshot(&runtime).await);
    assert!(snapshot.iter().all(|state| state.online));

    lifecycle_publish(
        &runtime,
        "source.b",
        PublishEvent {
            kind: EventKind::BotOffline,
            account: Some("shared".into()),
            payload: to_value(&BotOffline {
                account: "shared".into(),
                reason: "disconnected".into(),
            })
            .unwrap(),
        },
    )
    .await
    .unwrap();
    let snapshot = bot_snapshot(&runtime).await;
    assert!(
        snapshot
            .iter()
            .find(|state| state.source == "source.a")
            .unwrap()
            .online
    );
    assert!(
        !snapshot
            .iter()
            .find(|state| state.source == "source.b")
            .unwrap()
            .online
    );
    lifecycle_publish(
        &runtime,
        "source.b",
        online_publication("shared", "bot.b.updated"),
    )
    .await
    .unwrap();
    assert_eq!(bot_snapshot(&runtime).await.len(), 2);

    // Teardown invalidates the real provider state and rejects old host contexts.
    runtime.inner.stop(&source_a).await.unwrap();
    assert!(
        !bot_snapshot(&runtime)
            .await
            .iter()
            .find(|state| state.source == "source.a")
            .unwrap()
            .online
    );
    let error = runtime
        .inner
        .host(
            "source.a",
            old_token,
            Request::new("events.emit", online_publication("shared", "stale")).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    runtime.inner.prepare(&source_a).await.unwrap();
    runtime.inner.schedule(source_a).await;
    settled(&runtime).await;
    lifecycle_publish(
        &runtime,
        "source.a",
        online_publication("shared", "restarted"),
    )
    .await
    .unwrap();
    assert_eq!(bot_snapshot(&runtime).await.len(), 2);
    runtime.shutdown().await.unwrap();
    assert!(
        bot_snapshot(&runtime)
            .await
            .iter()
            .all(|state| !state.online)
    );
}

#[tokio::test]
async fn failed_start_and_failed_callbacks_leave_retained_accounts_offline() {
    for fail_start in [true, false] {
        let root = TempDir::new().unwrap();
        let descriptor = metadata("failing.source");
        let publication =
            encode(&Request::new("events.emit", online_publication("account", "bot")).unwrap())
                .unwrap();
        let start = format!(
            "i32.const 20000 i32.const {} call $call call $drop {}",
            publication.len(),
            if fail_start { "unreachable" } else { "" },
        );
        deploy(
            root.path(),
            "provider",
            &guest_program(
                &descriptor,
                EventDisposition::Continue,
                &start,
                Some("unreachable"),
                &publication,
            ),
        )
        .await;
        let runtime = Runtime::open(root.path().into()).await.unwrap();
        runtime.start().await.unwrap();
        settled(&runtime).await;
        let plugin = runtime.inner.plugin("failing.source").await.unwrap();
        if !fail_start {
            assert!(bot_snapshot(&runtime).await[0].online);
            assert!(
                runtime
                    .inner
                    .call(plugin.clone(), wasm_abi::SERVICE, Vec::new())
                    .await
                    .is_err()
            );
        }
        assert!(matches!(
            plugin.state.lock().await.0,
            ExtensionState::Failed
        ));
        let states = bot_snapshot(&runtime).await;
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].bot.id, "bot");
        assert!(!states[0].online);
        runtime.shutdown().await.unwrap();
        assert!(!bot_snapshot(&runtime).await[0].online);
    }
}

#[tokio::test]
async fn rejected_lifecycle_publications_do_not_mutate_snapshots_and_full_queues_do_not_block_teardown()
 {
    let root = TempDir::new().unwrap();
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    // Leave the dispatcher unstarted to deterministically saturate its bounded queue.
    runtime
        .inner
        .publish("source", online_publication("existing", "bot"))
        .await
        .unwrap();
    for _ in 0..255 {
        runtime
            .inner
            .publish(
                "source",
                PublishEvent {
                    kind: EventKind::Custom("fill.queue".into()),
                    account: None,
                    payload: Value::Null,
                },
            )
            .await
            .unwrap();
    }
    let error = runtime
        .inner
        .publish("source", online_publication("rejected", "phantom"))
        .await
        .unwrap_err();
    assert_eq!(error.code, "event_queue_unavailable");
    assert_eq!(bot_snapshot(&runtime).await.len(), 1);
    tokio::time::timeout(
        Duration::from_secs(1),
        runtime.inner.offline_source("source"),
    )
    .await
    .unwrap();
    assert!(!bot_snapshot(&runtime).await[0].online);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn lifecycle_publication_racing_stop_cannot_resurrect_a_source_or_deadlock() {
    let root = TempDir::new().unwrap();
    deploy(
        root.path(),
        "source",
        &guest(&metadata("source"), EventDisposition::Continue, ""),
    )
    .await;
    let runtime = Runtime::open(root.path().into()).await.unwrap();
    runtime.start().await.unwrap();
    settled(&runtime).await;
    let plugin = runtime.inner.plugin("source").await.unwrap();
    // Pause an accepted publication after it takes the provider-state lock.
    let bots = runtime.inner.bots.write().await;
    let publisher = {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            lifecycle_publish(&runtime, "source", online_publication("account", "bot")).await
        })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while plugin.state.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let stopper = {
        let runtime = runtime.clone();
        let plugin = plugin.clone();
        tokio::spawn(async move { runtime.inner.stop(&plugin).await })
    };
    drop(bots);
    tokio::time::timeout(Duration::from_secs(5), async {
        publisher.await.unwrap().unwrap();
        stopper.await.unwrap().unwrap();
    })
    .await
    .unwrap();
    let snapshot = bot_snapshot(&runtime).await;
    assert_eq!(snapshot.len(), 1);
    assert!(!snapshot[0].online);
    runtime.shutdown().await.unwrap();
}
