use crate::{
    control,
    native::{HostBridge, NativeInstance, NativeModule},
    storage::Storage,
    tasks::Tasks,
    wasm::{WasmInstance, WasmModule},
};
use atri_core::*;
use futures::{FutureExt, future::BoxFuture};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Mutex, RwLock, broadcast, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

const DEADLINE: Duration = Duration::from_secs(35);
const NOTICE_BYTES: usize = 64 * 1024;

#[derive(Clone, Default)]
struct ServiceGraph(Arc<std::sync::Mutex<BTreeMap<(String, String), usize>>>);
struct ServiceLease {
    graph: ServiceGraph,
    edge: (String, String),
}
impl ServiceGraph {
    fn enter(&self, caller: &str, provider: &str) -> Result<ServiceLease, ApiError> {
        let mut edges = self.0.lock().unwrap();
        let mut pending = vec![provider.to_string()];
        let mut visited = BTreeSet::new();
        while let Some(node) = pending.pop() {
            if node == caller {
                return Err(ApiError::new(
                    "service_cycle",
                    format!(
                        "service call {caller} -> {provider} would form an active dependency cycle"
                    ),
                ));
            }
            if visited.insert(node.clone()) {
                for ((from, to), _) in edges.iter() {
                    if from == &node {
                        pending.push(to.clone());
                    }
                }
            }
        }
        let edge = (caller.to_string(), provider.to_string());
        *edges.entry(edge.clone()).or_default() += 1;
        Ok(ServiceLease {
            graph: self.clone(),
            edge,
        })
    }
}
impl Drop for ServiceLease {
    fn drop(&mut self) {
        let mut edges = self.graph.0.lock().unwrap();
        if let Some(count) = edges.get_mut(&self.edge) {
            *count -= 1;
            if *count == 0 {
                edges.remove(&self.edge);
            }
        }
    }
}
pub(crate) fn check_notice(value: &Value) -> Result<(), ApiError> {
    fn visit(value: &Value, remaining: &mut usize, depth: usize) -> Result<(), ApiError> {
        let too_large = || {
            ApiError::new(
                "output_too_large",
                "Wasm metadata or notice exceeds the 64 KiB host allocation budget",
            )
        };
        if depth > 64 {
            return Err(too_large());
        }
        *remaining = remaining
            .checked_sub(std::mem::size_of::<Value>())
            .ok_or_else(too_large)?;
        match value {
            Value::Text(text) => {
                *remaining = remaining.checked_sub(text.len()).ok_or_else(too_large)?
            }
            Value::Bytes(bytes) => {
                *remaining = remaining.checked_sub(bytes.len()).ok_or_else(too_large)?
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, remaining, depth + 1)?;
                }
            }
            Value::Map(entries) => {
                for (key, value) in entries {
                    visit(key, remaining, depth + 1)?;
                    visit(value, remaining, depth + 1)?;
                }
            }
            Value::Tag(_, value) => visit(value, remaining, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
    let mut remaining = NOTICE_BYTES;
    visit(value, &mut remaining, 0)
}
#[derive(Clone)]
pub struct Runtime {
    pub(crate) inner: Arc<Inner>,
}
pub(crate) struct Inner {
    pub storage: Storage,
    pub root: PathBuf,
    plugins: RwLock<BTreeMap<String, Arc<Plugin>>>,
    services: RwLock<BTreeMap<(String, Option<String>), String>>,
    bots: RwLock<BTreeMap<(String, String), BotState>>,
    service_graph: ServiceGraph,
    tasks: Tasks,
    notices: broadcast::Sender<RuntimeNotice>,
    pub running: AtomicBool,
    closed: AtomicBool,
    event_id: AtomicU64,
    lifecycle: Mutex<()>,
    server: Mutex<Option<control::Server>>,
    events: mpsc::Sender<Event>,
    event_receiver: Mutex<Option<mpsc::Receiver<Event>>>,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
}
struct Plugin {
    metadata: ExtensionMetadata,
    module: Module,
    path: PathBuf,
    state: Mutex<(ExtensionState, Option<String>)>,
    instance: RwLock<Option<Instance>>,
    scope: Mutex<CancellationToken>,
    lifecycle: Mutex<()>,
    start: Mutex<Option<JoinHandle<()>>>,
}
enum Module {
    Native(Arc<NativeModule>),
    Wasm(Arc<WasmModule>),
}
#[derive(Clone)]
enum Instance {
    Native(Arc<NativeInstance>),
    Wasm(Arc<WasmInstance>),
}
impl Instance {
    async fn invoke(&self, op: u32, payload: Vec<u8>) -> Result<Value, ApiError> {
        match self {
            Self::Native(i) => i.invoke(op, payload).await,
            Self::Wasm(i) => i.invoke(op, payload).await,
        }
    }
}
impl Plugin {
    async fn info(&self, tasks: &Tasks) -> ExtensionInfo {
        let state = self.state.lock().await;
        ExtensionInfo {
            metadata: self.metadata.clone(),
            kind: match self.module {
                Module::Native(_) => "native",
                Module::Wasm(_) => "wasm",
            }
            .into(),
            path: self.path.display().to_string(),
            state: state.0.clone(),
            tasks: tasks.list(Some(&self.metadata.id)),
            error: state.1.clone(),
        }
    }
}
impl Runtime {
    pub async fn open(bot_dir: PathBuf) -> Result<Self, ApiError> {
        let storage = Storage::open(bot_dir)?;
        let root = storage.root().to_path_buf();
        tokio::fs::create_dir_all(root.join("extensions"))
            .await
            .map_err(ApiError::internal)?;
        let (notices, _) = broadcast::channel(128);
        let (events, event_receiver) = mpsc::channel(256);
        let runtime = Self {
            inner: Arc::new(Inner {
                storage,
                root,
                plugins: RwLock::new(BTreeMap::new()),
                services: RwLock::new(BTreeMap::new()),
                bots: RwLock::new(BTreeMap::new()),
                service_graph: ServiceGraph::default(),
                tasks: Tasks::default(),
                notices,
                running: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                event_id: AtomicU64::new(0),
                lifecycle: Mutex::new(()),
                server: Mutex::new(None),
                events,
                event_receiver: Mutex::new(Some(event_receiver)),
                dispatcher: Mutex::new(None),
            }),
        };
        runtime.inner.rescan().await?;
        Ok(runtime)
    }
    /// Inspect deployed artifact paths without loading extension code or owning
    /// the writer lock. Status queries must not compete with daemon startup.
    pub fn deployed_artifacts(bot_dir: &Path) -> Result<Vec<PathBuf>, ApiError> {
        let directory = bot_dir.join("extensions");
        if !directory.try_exists().map_err(ApiError::internal)? {
            return Ok(Vec::new());
        }
        discover(&directory)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeNotice> {
        self.inner.notices.subscribe()
    }
    pub async fn snapshot(&self) -> RuntimeSnapshot {
        self.inner.snapshot().await
    }
    pub async fn start(&self) -> Result<(), ApiError> {
        let _lock = self.inner.lifecycle.lock().await;
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(ApiError::new(
                "runtime_closed",
                "runtime has already shut down",
            ));
        }
        if self.inner.running.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        match control::serve(self.clone()).await {
            Ok(server) => *self.inner.server.lock().await = Some(server),
            Err(error) => {
                self.inner.running.store(false, Ordering::Release);
                return Err(error);
            }
        }
        let disabled = match self.inner.storage.disabled().await {
            Ok(disabled) => disabled,
            Err(error) => {
                self.inner.running.store(false, Ordering::Release);
                if let Some(server) = self.inner.server.lock().await.take() {
                    server.stop().await;
                }
                return Err(error);
            }
        };
        if let Some(mut receiver) = self.inner.event_receiver.lock().await.take() {
            let runtime = Arc::downgrade(&self.inner);
            *self.inner.dispatcher.lock().await = Some(tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    let Some(runtime) = runtime.upgrade() else {
                        break;
                    };
                    if let Err(error) = runtime.dispatch(event).await {
                        runtime.report("atri.runtime", error);
                    }
                }
            }));
        }
        let plugins: Vec<_> = self.inner.plugins.read().await.values().cloned().collect();
        // Prepare every callback target before starting any platform extension.
        let mut ready = Vec::new();
        for plugin in plugins {
            if !disabled.contains(&plugin.metadata.id) {
                match self.inner.prepare(&plugin).await {
                    Ok(()) => ready.push(plugin),
                    Err(e) => {
                        self.inner
                            .state(&plugin, ExtensionState::Failed, Some(e.to_string()))
                            .await
                    }
                }
            }
        }
        for plugin in ready {
            self.inner.schedule(plugin).await;
        }
        Ok(())
    }
    pub async fn shutdown(&self) -> Result<(), ApiError> {
        self.shutdown_inner(false).await
    }
    pub(crate) async fn shutdown_from_control(&self) -> Result<(), ApiError> {
        self.shutdown_inner(true).await
    }
    async fn shutdown_inner(&self, responding: bool) -> Result<(), ApiError> {
        let _lock = self.inner.lifecycle.lock().await;
        if self.inner.closed.swap(true, Ordering::AcqRel) {
            let plugins: Vec<_> = self.inner.plugins.read().await.values().cloned().collect();
            for plugin in plugins {
                if plugin.instance.read().await.is_some() {
                    return Err(ApiError::new(
                        "shutdown_failed",
                        format!("{} still has a retained instance", plugin.metadata.id),
                    ));
                }
            }
            return Ok(());
        }
        self.inner.running.store(false, Ordering::Release);
        let plugins: Vec<_> = self.inner.plugins.read().await.values().cloned().collect();
        let mut failures = Vec::new();
        for plugin in plugins {
            if let Err(error) = self.inner.stop(&plugin).await {
                failures.push(format!("{}: {error}", plugin.metadata.id));
            }
        }
        if let Some(job) = self.inner.dispatcher.lock().await.take() {
            job.abort();
            let _ = job.await;
        }
        if let Some(server) = self.inner.server.lock().await.take() {
            if responding {
                server.finish_response();
            } else {
                server.stop().await;
            }
        }
        self.inner.storage.set_control_metadata(None).await?;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ApiError::new("shutdown_failed", failures.join("; ")))
        }
    }
    pub async fn control(&self, request: Request) -> Result<Value, ApiError> {
        if request.method == "runtime.stop" {
            self.shutdown().await?;
            return Ok(Value::Null);
        }
        let _lifecycle = if request.method.starts_with("extensions.") {
            Some(self.inner.lifecycle.lock().await)
        } else {
            None
        };
        if self.inner.closed.load(Ordering::Acquire)
            && !matches!(
                request.method.as_str(),
                "runtime.status"
                    | "extensions.list"
                    | "extensions.info"
                    | "tasks.list"
                    | "bots.list"
            )
        {
            return Err(ApiError::new("runtime_closed", "runtime has shut down"));
        }
        self.inner.control(request).await
    }
}
impl Inner {
    async fn snapshot(&self) -> RuntimeSnapshot {
        let plugins: Vec<_> = self.plugins.read().await.values().cloned().collect();
        let mut extensions = Vec::with_capacity(plugins.len());
        for plugin in plugins {
            extensions.push(plugin.info(&self.tasks).await);
        }
        RuntimeSnapshot {
            extensions,
            running: self.running.load(Ordering::Acquire),
        }
    }
    async fn plugin(&self, id: &str) -> Result<Arc<Plugin>, ApiError> {
        self.plugins
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError::new("extension_not_found", id))
    }
    async fn state(&self, plugin: &Plugin, state: ExtensionState, error: Option<String>) {
        let unavailable = matches!(
            state,
            ExtensionState::Stopping | ExtensionState::Disabled | ExtensionState::Failed
        );
        let mut current = plugin.state.lock().await;
        *current = (state, error);
        if unavailable {
            self.offline_source(&plugin.metadata.id).await;
        }
        drop(current);
        let _ = self
            .notices
            .send(RuntimeNotice::State(plugin.info(&self.tasks).await));
    }
    async fn load(&self, path: &Path) -> Result<Arc<Plugin>, ApiError> {
        let (module, metadata) = if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
            let (m, d) = WasmModule::load(path).await?;
            (Module::Wasm(m), d)
        } else {
            let (m, d) = unsafe { NativeModule::load(path) }?;
            (Module::Native(m), d)
        };
        validate_metadata(&metadata)?;
        self.storage.prepare(&metadata).await?;
        Ok(Arc::new(Plugin {
            metadata,
            module,
            path: path.to_path_buf(),
            state: Mutex::new((ExtensionState::Disabled, None)),
            instance: RwLock::new(None),
            scope: Mutex::new(CancellationToken::new()),
            lifecycle: Mutex::new(()),
            start: Mutex::new(None),
        }))
    }
    async fn rescan(self: &Arc<Self>) -> Result<(), ApiError> {
        let paths = discover(&self.root.join("extensions"))?;
        let plugins = self.plugins.read().await;
        let known: BTreeSet<_> = plugins.values().map(|p| p.path.clone()).collect();
        let mut ids: BTreeSet<_> = plugins.keys().cloned().collect();
        drop(plugins);
        let mut added = Vec::new();
        // Discovery and metadata validation must finish before any candidate is
        // published or given a live host context.
        for path in paths {
            if !known.contains(&path) {
                let plugin = self.load(&path).await?;
                let id = &plugin.metadata.id;
                if !ids.insert(id.clone()) {
                    return Err(ApiError::new(
                        "duplicate_extension",
                        format!("duplicate extension id {id} at {}", path.display()),
                    ));
                }
                added.push(plugin);
            }
        }
        let running = self.running.load(Ordering::Acquire);
        let disabled = if running {
            self.storage.disabled().await?
        } else {
            BTreeSet::new()
        };
        {
            let mut plugins = self.plugins.write().await;
            for plugin in &added {
                plugins.insert(plugin.metadata.id.clone(), plugin.clone());
            }
        }
        if running {
            let mut ready = Vec::new();
            for plugin in &added {
                if !disabled.contains(&plugin.metadata.id) {
                    if let Err(error) = self.prepare(plugin).await {
                        let mut failures = vec![error.to_string()];
                        // No start callbacks have been scheduled. Stop every staged
                        // instance before removing it, retaining unresponsive native
                        // instances just as ordinary disable/shutdown does.
                        for candidate in &added {
                            match self.stop(candidate).await {
                                Ok(()) => {
                                    self.plugins.write().await.remove(&candidate.metadata.id);
                                }
                                Err(cleanup) => {
                                    failures.push(format!("{}: {cleanup}", candidate.metadata.id))
                                }
                            }
                        }
                        return Err(ApiError::new(error.code, failures.join("; ")));
                    }
                    ready.push(plugin.clone());
                }
            }
            for plugin in ready {
                self.schedule(plugin).await;
            }
        }
        Ok(())
    }
    async fn prepare(self: &Arc<Self>, plugin: &Arc<Plugin>) -> Result<(), ApiError> {
        let _lifecycle = plugin.lifecycle.lock().await;
        if matches!(plugin.state.lock().await.0, ExtensionState::Failed) {
            return Err(ApiError::new(
                "extension_failed",
                "failed instances require a successful stop before restart",
            ));
        }
        if plugin.instance.read().await.is_some() {
            return Ok(());
        }
        let cancel = CancellationToken::new();
        *plugin.scope.lock().await = cancel.clone();
        let bridge = HostBridge {
            runtime: Arc::downgrade(self),
            id: plugin.metadata.id.clone(),
            cancel,
            handle: tokio::runtime::Handle::current(),
        };
        let instance = match &plugin.module {
            Module::Native(m) => Instance::Native(m.create(bridge)?),
            Module::Wasm(m) => Instance::Wasm(m.create(bridge).await?),
        };
        *plugin.instance.write().await = Some(instance);
        self.state(plugin, ExtensionState::Starting, None).await;
        Ok(())
    }
    async fn schedule(self: &Arc<Self>, plugin: Arc<Plugin>) {
        let mut start = plugin.start.lock().await;
        if start.is_some() {
            return;
        }
        let runtime = self.clone();
        let target = plugin.clone();
        let span = tracing::info_span!("extension.callback",extension=%plugin.metadata.id,operation="start");
        let job = tokio::spawn(
            async move {
                let instance = target.instance.read().await.clone();
                let Some(instance) = instance else {
                    return;
                };
                let result =
                    tokio::time::timeout(DEADLINE, instance.invoke(wasm_abi::START, Vec::new()))
                        .await;
                let mut state = target.state.lock().await;
                if !matches!(state.0, ExtensionState::Starting) {
                    return;
                }
                *state = match result {
                    Ok(Ok(_)) => (ExtensionState::Running, None),
                    Ok(Err(e)) => (ExtensionState::Failed, Some(e.to_string())),
                    Err(_) => (
                        ExtensionState::Failed,
                        Some("extension start deadline exceeded".into()),
                    ),
                };
                let failed = matches!(state.0, ExtensionState::Failed);
                if failed {
                    target.scope.lock().await.cancel();
                    runtime.offline_source(&target.metadata.id).await;
                }
                drop(state);
                if failed {
                    runtime
                        .services
                        .write()
                        .await
                        .retain(|_, owner| owner != &target.metadata.id);
                }
                let _ = runtime
                    .notices
                    .send(RuntimeNotice::State(target.info(&runtime.tasks).await));
            }
            .instrument(span),
        );
        *start = Some(job);
    }
    async fn stop(self: &Arc<Self>, plugin: &Arc<Plugin>) -> Result<(), ApiError> {
        let _lifecycle = plugin.lifecycle.lock().await;
        let instance = plugin.instance.read().await.clone();
        self.state(plugin, ExtensionState::Stopping, None).await;
        self.services
            .write()
            .await
            .retain(|_, owner| owner != &plugin.metadata.id);
        plugin.scope.lock().await.cancel();
        let Some(instance) = instance else {
            self.tasks.clear(&plugin.metadata.id);
            self.state(plugin, ExtensionState::Disabled, None).await;
            return Ok(());
        };
        let stopped=tokio::time::timeout(DEADLINE,async {
            if let Some(mut job)=plugin.start.lock().await.take() {
                // A start callback is not allowed to keep shutdown from reaching stop.
                // Dropping its completion receiver does not unload or kill native code.
                if !job.is_finished() {job.abort();}
                let _=(&mut job).await;
            }
            instance.invoke(wasm_abi::STOP,Vec::new()).instrument(tracing::info_span!("extension.callback",extension=%plugin.metadata.id,operation="stop")).await?;
            if let Instance::Native(native)=&instance {native.destroy_after_stop().await;}
            Ok::<(),ApiError>(())
        }).await.unwrap_or_else(|_|Err(ApiError::new("extension_unresponsive", match &instance {
            Instance::Wasm(_) => "Wasm stop exceeded deadline; guest discarded",
            Instance::Native(_) => "stop exceeded deadline; instance and native code retained",
        })));
        match stopped {
            Ok(()) => {
                *plugin.instance.write().await = None;
                self.tasks.clear(&plugin.metadata.id);
                self.state(plugin, ExtensionState::Disabled, None).await;
                Ok(())
            }
            Err(error) if matches!(instance, Instance::Wasm(_)) => {
                // A component trap or abandoned callback invalidates its store.
                // Wasm has no native code/tasks to retain: disposal is always a
                // successful, deterministic stop, with the diagnostic preserved.
                *plugin.instance.write().await = None;
                self.tasks.clear(&plugin.metadata.id);
                self.state(plugin, ExtensionState::Disabled, Some(error.to_string()))
                    .await;
                Ok(())
            }
            Err(error) => {
                self.state(plugin, ExtensionState::Failed, Some(error.to_string()))
                    .await;
                Err(error)
            }
        }
    }
    fn call(
        self: &Arc<Self>,
        plugin: Arc<Plugin>,
        op: u32,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<Value, ApiError>> {
        let span =
            tracing::info_span!("extension.callback",extension=%plugin.metadata.id,operation=op);
        let runtime = self.clone();
        async move {
            let state=plugin.state.lock().await;
            if !matches!(state.0,ExtensionState::Running|ExtensionState::Starting) {return Err(ApiError::new("extension_unavailable",&plugin.metadata.id));}
            let instance=plugin.instance.read().await.clone().ok_or_else(ApiError::cancelled)?; drop(state);
            let cancel=plugin.scope.lock().await.clone();
            let result = tokio::select! {biased; _=cancel.cancelled()=>Err(ApiError::cancelled()),result=tokio::time::timeout(DEADLINE,instance.invoke(op,payload))=>result.unwrap_or_else(|_|Err(ApiError::new("extension_timeout","callback deadline exceeded")))};
            if matches!(&instance, Instance::Wasm(wasm) if wasm.is_failed()) {
                runtime.state(&plugin, ExtensionState::Failed, result.as_ref().err().map(ToString::to_string)).await;
                cancel.cancel();
                runtime.services.write().await.retain(|_, owner| owner != &plugin.metadata.id);
            }
            result
        }.instrument(span).boxed()
    }
    async fn bots(&self) -> Vec<BotState> {
        self.bots.read().await.values().cloned().collect()
    }
    fn event(&self, source: &str, publish: PublishEvent) -> Event {
        Event {
            id: self.event_id.fetch_add(1, Ordering::Relaxed) + 1,
            source: source.into(),
            time_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            kind: publish.kind,
            account: publish.account,
            payload: publish.payload,
        }
    }
    async fn offline_source(&self, source: &str) {
        // Never await queue capacity: this can run from the event dispatcher itself.
        // The retained snapshot is authoritative even if a saturated queue rejects a notice.
        let mut bots = self.bots.write().await;
        for state in bots
            .values_mut()
            .filter(|state| state.source == source && state.online)
        {
            state.online = false;
            let offline = BotOffline {
                account: state.bot.account.clone(),
                reason: "source extension unavailable".into(),
            };
            let payload = match to_value(&offline) {
                Ok(payload) => payload,
                Err(error) => {
                    self.report(source, error);
                    continue;
                }
            };
            let event = self.event(
                source,
                PublishEvent {
                    kind: EventKind::BotOffline,
                    account: Some(offline.account),
                    payload,
                },
            );
            if let Err(error) = self.events.try_send(event.clone()) {
                self.report(
                    source,
                    ApiError::new("event_queue_unavailable", error.to_string()),
                );
            }
            let _ = self.notices.send(RuntimeNotice::Event(event));
        }
    }
    async fn publish(
        self: &Arc<Self>,
        source: &str,
        publish: PublishEvent,
    ) -> Result<(), ApiError> {
        let (online, offline) = match &publish.kind {
            EventKind::BotOnline => (Some(from_value::<BotOnline>(&publish.payload)?.bot), None),
            EventKind::BotOffline => (
                None,
                Some(from_value::<BotOffline>(&publish.payload)?.account),
            ),
            _ => (None, None),
        };
        let account = online
            .as_ref()
            .map(|bot| bot.account.as_str())
            .or(offline.as_deref());
        if let (Some(envelope), Some(account)) = (publish.account.as_deref(), account) {
            if envelope != account {
                return Err(ApiError::new(
                    "invalid_bot_account",
                    "lifecycle payload and event account differ",
                ));
            }
        }
        // Reserve before mutating state, so rejected publications leave no phantom accounts.
        let permit = self
            .events
            .try_reserve()
            .map_err(|error| ApiError::new("event_queue_unavailable", error.to_string()))?;
        // Lifecycle publishers hold their plugin state lock before this lock. No code here
        // takes plugin/service locks or awaits callbacks, and snapshots only take this lock.
        let mut bots = self.bots.write().await;
        if let Some(bot) = online {
            bots.insert(
                (source.to_owned(), bot.account.clone()),
                BotState {
                    source: source.to_owned(),
                    bot,
                    online: true,
                },
            );
        } else if let Some(account) = offline {
            if let Some(state) = bots.get_mut(&(source.to_owned(), account)) {
                state.online = false;
            }
        }
        let event = self.event(source, publish);
        permit.send(event.clone());
        let _ = self.notices.send(RuntimeNotice::Event(event));
        Ok(())
    }
    async fn dispatch(self: &Arc<Self>, event: Event) -> Result<(), ApiError> {
        let plugins: Vec<_> = self.plugins.read().await.values().cloned().collect();
        let mut listeners = Vec::new();
        let mut observers = Vec::new();
        for plugin in plugins {
            for sub in &plugin.metadata.subscriptions {
                if sub.kind == event.kind
                    || matches!(&sub.kind, EventKind::Custom(name) if name == "*")
                {
                    if sub.observer || !matches!(event.kind, EventKind::MessageReceived) {
                        observers.push((sub.priority, plugin.clone()));
                    } else {
                        listeners.push((sub.priority, plugin.clone()));
                    }
                    break;
                }
            }
        }
        listeners.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
        observers.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
        let bytes = encode(&event)?;
        for (_, plugin) in listeners {
            match self
                .call(plugin.clone(), wasm_abi::EVENT, bytes.clone())
                .await
                .and_then(|v| from_value::<EventDisposition>(&v))
            {
                Ok(EventDisposition::Stop) => break,
                Ok(_) => {}
                Err(e) => self.report(&plugin.metadata.id, e),
            }
        }
        for (_, plugin) in observers {
            if let Err(error) = self
                .call(plugin.clone(), wasm_abi::EVENT, bytes.clone())
                .await
            {
                self.report(&plugin.metadata.id, error);
            }
        }
        Ok(())
    }
    fn report(&self, id: &str, error: ApiError) {
        let message = if error.code.len().saturating_add(error.message.len()) > NOTICE_BYTES {
            ApiError::new("output_too_large", "extension error exceeds notice budget").to_string()
        } else {
            error.to_string()
        };
        let _ = self.notices.send(RuntimeNotice::Log {
            extension: id.into(),
            record: LogRecord {
                level: LogLevel::Error,
                message,
                fields: BTreeMap::new(),
            },
        });
    }
    pub fn host(
        self: &Arc<Self>,
        id: &str,
        cancel: CancellationToken,
        request: Request,
    ) -> BoxFuture<'static, Result<Value, ApiError>> {
        let runtime = self.clone();
        let id = id.to_string();
        let span = tracing::debug_span!("extension.host",extension=%id,method=%request.method);
        async move {
            let plugin=runtime.plugin(&id).await?;
            let method=request.method.as_str();
            let capability=match method {
                "log"=>Some("log"),"output.table"=>Some("output.table"),"interaction"=>Some("interaction"),"events.emit"=>Some("events.emit"),
                "services.register"|"services.unregister"=>Some("services.register"),"services.call"=>Some("services.call"),
                "bots.list"=>Some("bots.read"),
                "config.load"|"config.document.read"=>Some("config.read"),"config.store"|"config.document.write"=>Some("config.write"),
                "storage.read"=>Some("storage.read"),"storage.write"=>Some("storage.write"),"tasks.sleep"|"tasks.started"|"tasks.finished"=>Some("tasks.spawn"),
                "paths"|"tasks.cancelled"=>None,_=>return Err(ApiError::new("unknown_method",method)),
            };
            if let Some(cap)=capability {if !plugin.metadata.capabilities.iter().any(|v|v==cap||v=="*") {return Err(ApiError::new("capability_denied",cap));}}
            if cancel.is_cancelled() && !matches!(method,"tasks.finished"|"tasks.cancelled"|"log"|"paths"|"config.load"|"config.store"|"config.document.read"|"config.document.write"|"storage.read"|"storage.write"|"services.unregister") {return Err(ApiError::cancelled());}
            if matches!(plugin.module,Module::Wasm(_)) && matches!(method,"log"|"output.table"|"interaction"|"events.emit") {check_notice(&request.params)?;}
            let spec=&plugin.metadata.configuration; let p=&request.params;
            match method {
                "paths"=>{let mut paths=runtime.storage.prepare(&plugin.metadata).await?;if matches!(plugin.module,Module::Wasm(_)) {paths=ExtensionPaths {root:format!("atri://{id}"),data:format!("atri://{id}/data"),workspace:format!("atri://{id}/workspace")};}to_value(&paths)},
                "log"=>{let record=from_value(p)?;let _=runtime.notices.send(RuntimeNotice::Log {extension:id,record});Ok(Value::Null)},
                "output.table"=>{let table=from_value(p)?;let _=runtime.notices.send(RuntimeNotice::Table {extension:id,table});Ok(Value::Null)},
                "interaction"=>{let request=from_value(p)?;let _=runtime.notices.send(RuntimeNotice::Interaction {extension:id,request});Ok(Value::Null)},
                "bots.list"=>to_value(&runtime.bots().await),
                "events.emit"=>{
                    // Recheck under the same state lock as teardown; a host call that began
                    // before cancellation must not resurrect an account after source failure.
                    let state=plugin.state.lock().await;
                    if cancel.is_cancelled() || !matches!(state.0,ExtensionState::Starting|ExtensionState::Running) {
                        return Err(ApiError::cancelled());
                    }
                    runtime.publish(&id,from_value(p)?).await?;
                    Ok(Value::Null)
                },
                "services.register"|"services.unregister"=>{
                    let binding:ServiceBinding=from_value(p)?;
                    if !plugin.metadata.services.iter().any(|s|s.name==binding.service) {return Err(ApiError::new("undeclared_service",binding.service));}
                    let key=(binding.service,binding.account);let mut services=runtime.services.write().await;
                    if method=="services.register" {if services.get(&key).is_some_and(|owner|owner!=&id) {return Err(ApiError::new("service_conflict",key.0));}services.insert(key,id);}
                    else if services.get(&key)==Some(&id) {services.remove(&key);} Ok(Value::Null)
                },
                "services.call"=>{
                    let mut call:ServiceCall=from_value(p)?;call.caller=id;call.caller_kind=match plugin.module {Module::Native(_)=>CallerKind::Native,Module::Wasm(_)=>CallerKind::Wasm};
                    let owner=runtime.services.read().await.get(&(call.service.clone(),call.account.clone())).cloned().ok_or_else(||ApiError::new("service_not_found",&call.service))?;
                    let provider=runtime.plugin(&owner).await?;
                    if !provider.metadata.services.iter().any(|s|s.name==call.service&&s.methods.contains(&call.method)) {return Err(ApiError::new("unknown_service_method",call.method));}
                    let _dependency=runtime.service_graph.enter(&call.caller,&owner)?;
                    runtime.call(provider,wasm_abi::SERVICE,encode(&call)?).instrument(tracing::info_span!("extension.service",caller=%call.caller,provider=%owner,service=%call.service,method=%call.method)).await
                },
                "config.load"=>to_value(&runtime.storage.config_load(&id,spec,from_value(p)?).await?),
                "config.store"=>to_value(&runtime.storage.config_store(&id,spec,from_value(p)?).await?),
                "config.document.read"=>to_value(&runtime.storage.config_document(&id,spec).await?),
                "config.document.write"=>to_value(&runtime.storage.config_document_store(&id,spec,from_value(p)?).await?),
                "storage.read"=>Ok(Value::Bytes(runtime.storage.read(&id,from_value(p)?).await?)),
                "storage.write"=>{runtime.storage.write(&id,from_value(p)?).await?;Ok(Value::Null)},
                "tasks.cancelled"=>Ok(Value::Bool(cancel.is_cancelled())),
                "tasks.started"|"tasks.finished"=>{
                    if matches!(plugin.module,Module::Wasm(_)) {return Err(ApiError::unsupported("Wasm has no native background-task notification API"));}
                    runtime.tasks.native(&id,from_value(p)?,method=="tasks.finished")?;Ok(Value::Null)
                },
                "tasks.sleep"=>{
                    let sleep:SleepRequest=from_value(p)?;let token=cancel.child_token();let task=runtime.tasks.begin(&id,"sleep",token.clone());
                    let _task=runtime.tasks.finish_on_drop(task);
                    let result=tokio::select! {_=token.cancelled()=>Err(ApiError::cancelled()),_=tokio::time::sleep(Duration::from_millis(sleep.milliseconds))=>Ok(Value::Null)};
                    result
                },
                _=>Err(ApiError::new("unknown_method",method)),
            }
        }.instrument(span).boxed()
    }
    async fn control(self: &Arc<Self>, request: Request) -> Result<Value, ApiError> {
        let p = &request.params;
        match request.method.as_str() {
            "runtime.status" => to_value(&self.snapshot().await),
            "extensions.list" => to_value(&self.snapshot().await.extensions),
            "bots.list" => to_value(&self.bots().await),
            "services.call" => {
                let mut call: ServiceCall = from_value(p)?;
                call.caller = "atri.control".into();
                call.caller_kind = CallerKind::Runtime;
                let owner = self
                    .services
                    .read()
                    .await
                    .get(&(call.service.clone(), call.account.clone()))
                    .cloned()
                    .ok_or_else(|| ApiError::new("service_not_found", &call.service))?;
                let provider = self.plugin(&owner).await?;
                if !provider
                    .metadata
                    .services
                    .iter()
                    .any(|s| s.name == call.service && s.methods.contains(&call.method))
                {
                    return Err(ApiError::new("unknown_service_method", call.method));
                }
                // Only extension-to-extension calls form dependency graph edges.
                self.call(provider,wasm_abi::SERVICE,encode(&call)?).instrument(tracing::info_span!("extension.service",caller=%call.caller,provider=%owner,service=%call.service,method=%call.method)).await
            }
            "extensions.info" => {
                let a: Id = from_value(p)?;
                to_value(&self.plugin(&a.id).await?.info(&self.tasks).await)
            }
            "extensions.rescan" => {
                self.rescan().await?;
                to_value(&self.snapshot().await.extensions)
            }
            "extensions.enable" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                self.storage.set_enabled(&a.id, &true).await?;
                if self.running.load(Ordering::Acquire) {
                    self.prepare(&plugin).await?;
                    self.schedule(plugin.clone()).await;
                }
                to_value(&plugin.info(&self.tasks).await)
            }
            "extensions.disable" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                self.storage.set_enabled(&a.id, &false).await?;
                self.stop(&plugin).await?;
                to_value(&plugin.info(&self.tasks).await)
            }
            "extensions.install" => {
                #[derive(Deserialize)]
                struct Install {
                    path: PathBuf,
                }
                let a: Install = from_value(p)?;
                self.install(a.path).await
            }
            "extensions.remove" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                self.stop(&plugin).await?;
                tokio::fs::remove_file(&plugin.path)
                    .await
                    .map_err(ApiError::internal)?;
                self.plugins.write().await.remove(&a.id);
                Ok(Value::Null)
            }
            "tasks.list" => to_value(&self.tasks.list(None)),
            "tasks.cancel" => {
                #[derive(Deserialize)]
                struct Cancel {
                    id: u64,
                }
                let a: Cancel = from_value(p)?;
                if let Some(owner) = self.tasks.cancel(a.id)? {
                    let plugin = self.plugin(&owner).await?;
                    let call = ServiceCall {
                        service: "atri.sdk.tasks".into(),
                        method: "cancel".into(),
                        account: None,
                        args: to_value(&TaskId { id: a.id })?,
                        caller: "atri.runtime".into(),
                        caller_kind: CallerKind::Runtime,
                    };
                    self.call(plugin, wasm_abi::SERVICE, encode(&call)?).await?;
                }
                Ok(Value::Null)
            }
            "config.get" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                to_value(
                    &self
                        .storage
                        .config_load(
                            &a.id,
                            &plugin.metadata.configuration,
                            ConfigLoad {
                                defaults: None,
                                format: None,
                            },
                        )
                        .await?,
                )
            }
            "config.set" => {
                #[derive(Deserialize)]
                struct Set {
                    id: String,
                    value: Value,
                    key: Option<String>,
                    expected_revision: Option<String>,
                    format: Option<String>,
                }
                let a: Set = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                let spec = &plugin.metadata.configuration;
                let result = if let Some(key) = a.key {
                    self.storage
                        .config_set_key(&a.id, spec, &key, a.value, a.expected_revision)
                        .await?
                } else {
                    self.storage
                        .config_store(
                            &a.id,
                            spec,
                            ConfigStore {
                                value: a.value,
                                expected_revision: a.expected_revision,
                                format: a.format,
                            },
                        )
                        .await?
                };
                to_value(&result)
            }
            "config.document.get" | "document.get" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                to_value(
                    &self
                        .storage
                        .config_document(&a.id, &plugin.metadata.configuration)
                        .await?,
                )
            }
            "config.document.set" | "document.set" | "config.import" | "import" => {
                let a: Id = from_value(p)?;
                let plugin = self.plugin(&a.id).await?;
                let document: ConfigDocumentStore = from_value(p)?;
                if matches!(request.method.as_str(), "config.import" | "import") {
                    to_value(
                        &self
                            .storage
                            .config_import(&a.id, &plugin.metadata.configuration, document)
                            .await?,
                    )
                } else {
                    to_value(
                        &self
                            .storage
                            .config_document_store(&a.id, &plugin.metadata.configuration, document)
                            .await?,
                    )
                }
            }
            _ => Err(ApiError::new("unknown_method", request.method)),
        }
    }
    async fn install(self: &Arc<Self>, source: PathBuf) -> Result<Value, ApiError> {
        let source = tokio::fs::canonicalize(source)
            .await
            .map_err(ApiError::internal)?;
        let candidates = if source.is_dir() {
            discover(&source)?
        } else {
            vec![source]
        };
        if candidates.len() != 1 {
            return Err(ApiError::new(
                "install_artifact",
                "install requires exactly one native or Wasm artifact",
            ));
        }
        let source = &candidates[0];
        if !is_artifact(source) {
            return Err(ApiError::new(
                "install_artifact",
                "unsupported artifact extension",
            ));
        }
        let plugin = self.load(source).await?;
        let id = plugin.metadata.id.clone();
        if self.plugins.read().await.contains_key(&id) {
            return Err(ApiError::new("extension_exists", &id));
        }
        let role = match plugin.metadata.role {
            ExtensionRole::Adapter => "adapter",
            ExtensionRole::Service => "service",
        };
        let directory = self.root.join("extensions").join(role);
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(ApiError::internal)?;
        let destination = directory.join(
            source
                .file_name()
                .ok_or_else(|| ApiError::new("install_artifact", "missing artifact filename"))?,
        );
        let staged = directory.join(format!(".{}.install", uuid::Uuid::new_v4()));
        if let Err(error) = tokio::fs::copy(source, &staged).await {
            let _ = tokio::fs::remove_file(&staged).await;
            return Err(ApiError::internal(error));
        }
        // Publishing a same-directory hard link is atomic and never replaces an
        // existing artifact, unlike rename on platforms which allow overwrites.
        let published = tokio::fs::hard_link(&staged, &destination).await;
        let _ = tokio::fs::remove_file(&staged).await;
        if let Err(error) = published {
            return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                ApiError::new(
                    "install_destination_exists",
                    destination.display().to_string(),
                )
            } else {
                ApiError::internal(error)
            });
        }
        let mut plugin = Arc::try_unwrap(plugin)
            .map_err(|_| ApiError::internal("unexpected shared install metadata"))?;
        plugin.path = destination;
        let plugin = Arc::new(plugin);
        self.storage.set_enabled(&id, &false).await?;
        self.plugins
            .write()
            .await
            .insert(plugin.metadata.id.clone(), plugin.clone());
        to_value(&plugin.info(&self.tasks).await)
    }
}
#[derive(Deserialize)]
struct Id {
    id: String,
}
#[derive(Serialize)]
struct TaskId {
    id: u64,
}
pub(crate) fn validate_metadata(metadata: &ExtensionMetadata) -> Result<(), ApiError> {
    if metadata.api_version != API_VERSION {
        return Err(ApiError::new(
            "extension_abi_mismatch",
            "metadata API version mismatch",
        ));
    }
    if metadata.id.is_empty() || !metadata.id.is_ascii() {
        return Err(ApiError::new("invalid_extension_id", &metadata.id));
    }
    let mut services = BTreeSet::new();
    for service in &metadata.services {
        if service.name.is_empty() || !services.insert(&service.name) || service.methods.is_empty()
        {
            return Err(ApiError::new(
                "invalid_metadata",
                "service names must be unique and declare methods",
            ));
        }
    }
    Ok(())
}
fn is_artifact(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("so" | "dylib" | "dll" | "wasm")
    )
}
fn discover(root: &Path) -> Result<Vec<PathBuf>, ApiError> {
    let mut paths = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(ApiError::internal)?;
        if entry.file_type().is_file() && is_artifact(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    Ok(paths)
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
