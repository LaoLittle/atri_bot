#![feature(once_cell)]

extern crate core;

use std::sync::OnceLock;
use std::time::Duration;

use dashmap::DashMap;
use ricq::client::NetworkStatus;
use ricq::msg::elem::Text;
use ricq::structs::GroupMemberInfo;

use tokio::runtime;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tracing::error;

use crate::client::Client;

use crate::event::listener::Listener;
use crate::event::Event;
use crate::service::listener::ListenerWorker;
use crate::service::plugin_manager::PluginManager;

pub mod channel;
pub mod client;
pub mod config;
pub mod contact;
pub mod data;
pub mod error;
pub mod event;
pub mod macros;
pub mod message;
pub mod plugin;
pub mod service;
pub mod terminal;

pub struct Atri {
    pub runtime: Runtime,
    //listener_runtime: Runtime,
    //listener_worker: ListenerWorker,
    pub disconnect_provider: JoinHandle<()>,
    pub plugin_manager: PluginManager,
}

impl Atri {
    pub fn new() -> Self {
        let runtime = runtime::Builder::new_multi_thread()
            .thread_name("GlobalRuntime")
            .enable_all()
            .build()
            .unwrap();

        let provider = runtime.spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(60 * 2)).await;

                for client in Client::list() {
                    if client.network_status() == 4 {
                        global_status().remove_client(client.id());
                        error!("{}因网络原因掉线, 尝试重连", client);

                        client.request_client().stop(NetworkStatus::NetworkOffline);

                        if let Err(e) = client.start().await {
                            error!("重连失败: {}", e);
                        }

                        if let Err(e) = client.try_login().await {
                            error!("登录失败: {}", e);
                        }
                    }
                }
            }
        });

        Self {
            runtime,
            //listener_runtime,
            //listener_worker,
            disconnect_provider: provider,
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
            .thread_name("Global-Listener-Executor")
            .enable_all()
            .build()
            .unwrap();

        Self {
            clients: DashMap::new(),
            listener_worker: ListenerWorker::new_with_runtime(listener_runtime),
        }
    }

    pub fn bots(&self) -> Vec<Client> {
        let mut bots = vec![];
        for bot in self.clients.iter() {
            let c = bot.clone();
            bots.push(c);
        }

        bots
    }

    pub fn listener_worker(&self) -> &ListenerWorker {
        &self.listener_worker
    }

    pub(crate) fn add_client(&self, bot: Client) -> Option<Client> {
        self.clients.insert(bot.id(), bot)
    }

    pub(crate) fn remove_client(&self, bot: i64) -> Option<Client> {
        self.clients.remove(&bot).map(|(_, bot)| bot)
    }
}

impl Default for AtriGlobalStatus {
    fn default() -> Self {
        Self::new()
    }
}
