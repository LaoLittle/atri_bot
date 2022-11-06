#![feature(once_cell)]

extern crate core;

use std::sync::OnceLock;

use dashmap::DashMap;
use ricq::msg::elem::Text;
use ricq::structs::GroupMemberInfo;

use tokio::runtime;
use tokio::runtime::Runtime;

use crate::client::Client;

use crate::event::listener::Listener;
use crate::event::Event;
use crate::service::listeners::ListenerWorker;
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
    pub global_runtime: Runtime,
    //listener_runtime: Runtime,
    //listener_worker: ListenerWorker,
    pub plugin_manager: PluginManager,
}

impl Atri {
    pub fn new() -> Self {
        let global_runtime = runtime::Builder::new_multi_thread()
            .thread_name("GlobalRuntime")
            .enable_all()
            .build()
            .unwrap();

        let _listener_runtime = runtime::Builder::new_multi_thread()
            .worker_threads(8)
            .thread_name("Listeners")
            .enable_all()
            .build()
            .unwrap();

        let _listener_worker = ListenerWorker::new();

        Self {
            global_runtime,
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

static APP: OnceLock<App> = OnceLock::new();

pub fn get_app() -> &'static App {
    APP.get_or_init(App::new)
}

static LISTENER_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn get_listener_runtime() -> &'static Runtime {
    LISTENER_RUNTIME.get_or_init(|| {
        runtime::Builder::new_multi_thread()
            .worker_threads(8)
            .thread_name("Listeners")
            .enable_all()
            .build()
            .unwrap()
    })
}

pub struct App {
    clients: DashMap<i64, Client>,
}

impl App {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
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

    pub(crate) fn add_client(&self, bot: Client) -> Option<Client> {
        self.clients.insert(bot.id(), bot)
    }

    pub(crate) fn remove_bot(&self, bot: i64) -> Option<Client> {
        self.clients.remove(&bot).map(|(_, bot)| bot)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
