#![feature(once_cell)]

extern crate core;

use std::mem;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use ricq::msg::elem::Text;
use ricq::msg::MessageChain;
use ricq::structs::GroupMemberInfo;

use tokio::runtime;
use tokio::runtime::Runtime;

use crate::bot::Bot;

use crate::event::listener::Listener;
use crate::event::Event;
use crate::service::listeners::ListenerWorker;
use crate::service::plugin::PluginManager;

pub mod bot;
pub mod channel;
pub mod config;
pub mod contact;
pub mod data;
pub mod event;
pub mod fun;
pub mod macros;
pub mod message;
pub mod plugin;
pub mod service;

pub struct Atri {
    global_runtime: Runtime,
    //listener_runtime: Runtime,
    //listener_worker: ListenerWorker,
    plugin_manager: PluginManager,
}

impl Atri {
    pub fn new() -> Self {
        let global_runtime = runtime::Builder::new_multi_thread()
            .thread_name("GlobalRuntime")
            .enable_all()
            .build()
            .unwrap();

        let listener_runtime = runtime::Builder::new_multi_thread()
            .worker_threads(8)
            .thread_name("Listeners")
            .enable_all()
            .build()
            .unwrap();

        let listener_worker = ListenerWorker::new();

        Self {
            global_runtime,
            //listener_runtime,
            //listener_worker,
            plugin_manager: PluginManager::new(),
        }
    }

    pub fn plugin_manager(&self) -> &PluginManager {
        &self.plugin_manager
    }
}

static APP: OnceLock<App> = OnceLock::new();

pub fn get_app() -> &'static App {
    APP.get_or_init(App::new)
}

static ASYNC_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn get_runtime() -> &'static Runtime {
    ASYNC_RUNTIME.get_or_init(|| {
        runtime::Builder::new_multi_thread()
            .thread_name("GlobalRuntime")
            .enable_all()
            .build()
            .unwrap()
    })
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
    bots: DashMap<i64, Bot>,
    group_bot: DashMap<i64, i64>,
    group_members_info: DashMap<i64, Arc<GroupMemberInfo>>,
    http_client: reqwest::Client,
}

impl App {
    pub fn new() -> Self {
        Self {
            bots: DashMap::new(),
            group_bot: DashMap::new(),
            group_members_info: DashMap::new(),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn bots(&self) -> Vec<Bot> {
        let mut bots = vec![];
        for bot in self.bots.iter() {
            let c = bot.clone();
            bots.push(c);
        }

        bots
    }

    pub fn group_bot(&self, group_id: i64) -> Option<i64> {
        self.group_bot.get(&group_id).map(|r| *r.value())
    }

    pub fn set_group_bot(&self, group_id: i64, bot_id: i64) -> Option<i64> {
        self.group_bot.insert(group_id, bot_id)
    }

    pub fn check_group_bot(&self, bot_id: i64, group_id: i64) -> bool {
        let group_bot = self.group_bot(group_id);

        if let Some(id) = group_bot {
            if id != bot_id {
                return false;
            }
        } else {
            get_app().set_group_bot(group_id, bot_id);
        }

        true
    }

    pub(crate) fn add_bot(&self, bot: Bot) -> Option<Bot> {
        self.bots.insert(bot.id(), bot)
    }

    pub(crate) fn remove_bot(&self, bot: i64) -> Option<Bot> {
        self.bots.remove(&bot).map(|(_, bot)| bot)
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }
}

pub fn app_receiver() {}

pub fn main_handler() {
    let guard = Listener::listening_on_always(|e: Event| async move {
        match e {
            Event::GroupMessageEvent(e) => {
                let s = e.message().elements.to_string();
                match &*s {
                    "萝卜子列表" => {
                        let app = get_app();
                        let bots = &app.bots;

                        let mut s = String::from("在线的萝卜子\n");
                        for bot in bots.iter() {
                            s.push_str(&format!(
                                "{0}: {1}",
                                bot.client().account_info.read().await.nickname,
                                bot.client().uin().await
                            ));
                            s.push('\n');
                        }
                        s.pop();

                        let chain = MessageChain::new(Text::new(s));

                        e.group().send_message(chain).await.expect("Error");
                    }
                    _ => {}
                };
            }
            _ => {}
        }
    })
    .start();

    mem::forget(guard);
}
