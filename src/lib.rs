#![feature(once_cell)]
#![feature(string_leak)]
#![feature(new_uninit)]

use dashmap::DashMap;
use ricq::msg::elem::Text;
use ricq::structs::GroupMemberInfo;
use std::sync::OnceLock;

use crate::client::Client;
use tokio::runtime;
use tokio::runtime::Runtime;

use crate::event::listener::Listener;
use crate::event::Event;
use crate::service::listener::ListenerWorker;
use crate::service::plugin::PluginManager;

pub mod channel;
pub mod client;
pub mod config;
pub mod contact;
//pub mod data;
pub mod error;
pub mod event;
pub mod macros;
pub mod message;
pub mod plugin;
pub mod service;
pub mod signal;
pub mod terminal;

pub struct Atri {
    pub runtime: Runtime,
    //listener_runtime: Runtime,
    //listener_worker: ListenerWorker,
    pub plugin_manager: PluginManager,
}

impl Atri {
    pub fn new() -> Self {
        let mut builder = runtime::Builder::new_multi_thread();

        let runtime = builder
            .thread_name("GlobalRuntime")
            .enable_all()
            .build()
            .expect("cannot create runtime");

        Self {
            runtime,
            //listener_runtime,
            //listener_worker,
            plugin_manager: PluginManager::new(),
        }
    }
}

impl Default for Atri {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AtriGlobalStatus {
    clients: DashMap<i64, Client>,
    listener_worker: ListenerWorker,
    //commands: std::sync::RwLock<HashMap<String, CommandAction>>,
}

static ATRI_GLOBAL_STATUS: OnceLock<AtriGlobalStatus> = OnceLock::new();

pub fn global_status() -> &'static AtriGlobalStatus {
    ATRI_GLOBAL_STATUS.get_or_init(AtriGlobalStatus::new)
}

pub fn global_listener_runtime() -> &'static Runtime {
    global_status().listener_worker().runtime()
}

pub fn global_listener_worker() -> &'static ListenerWorker {
    global_status().listener_worker()
}

impl AtriGlobalStatus {
    pub fn new() -> Self {
        let listener_runtime = runtime::Builder::new_multi_thread()
            .worker_threads(8)
            .thread_name("GlobalListenerExecutor")
            .enable_all()
            .build()
            .unwrap();

        Self {
            clients: DashMap::new(),
            listener_worker: ListenerWorker::new_with_runtime(listener_runtime),
            //commands: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn clients(&self) -> Vec<Client> {
        let mut clients = vec![];
        for client in self.clients.iter() {
            let c = client.clone();
            clients.push(c);
        }

        clients
    }

    pub fn listener_worker(&self) -> &ListenerWorker {
        &self.listener_worker
    }

    pub(crate) fn add_client(&self, client: Client) -> Option<Client> {
        self.clients.insert(client.id(), client)
    }

    pub(crate) fn remove_client(&self, client_id: i64) -> Option<Client> {
        self.clients.remove(&client_id).map(|(_, client)| client)
    }

    pub fn close_clients(&self) {
        for client in self.clients() {
            client.close();
            self.remove_client(client.id());
        }
    }
}

impl Default for AtriGlobalStatus {
    fn default() -> Self {
        Self::new()
    }
}
