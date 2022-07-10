#![feature(once_cell)]

extern crate core;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use ricq::handler::QEvent;
use ricq::msg::elem::Text;
use ricq::msg::MessageChain;

use tokio::sync::{RwLock};

use crate::bot::Bot;
use crate::channel::global_receiver;

pub mod bot;
pub mod channel;
pub mod config;
pub mod contact;
pub mod event;
pub mod fun;
pub mod service;
pub mod macros;
pub mod data;

static APP: OnceLock<App> = OnceLock::new();

pub fn get_app() -> &'static App {
    APP.get_or_init(App::new)
}

pub struct App {
    bots: RwLock<Vec<Bot>>,
    group_bot: RwLock<HashMap<i64, i64>>,
    http_client: reqwest::Client,
}

impl App {
    pub fn new() -> Self {
        Self {
            bots: RwLock::new(vec![]),
            group_bot: RwLock::new(HashMap::new()),
            http_client: reqwest::Client::new()
        }
    }

    pub fn bots(&self) -> &RwLock<Vec<Bot>> {
        &self.bots
    }

    pub async fn bots_clone(&self) -> Vec<Bot> {
        self.bots.read().await.clone()
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }
}

pub fn app_receiver() {

}

pub async fn main_handler() {
    let mut rx = global_receiver();

    while let Ok(e) = rx.recv().await {
        tokio::spawn(async move {
            match e {
                QEvent::GroupMessage(e) => {
                    let group_id = e.inner.group_code;
                    let bot_id = e.client.uin().await;

                    let group_bot = {
                        let lock = get_app().group_bot.read().await;
                        lock.get(&group_id).map(|id| *id)
                    };

                    if let Some(id) = group_bot {
                        if id != bot_id { return; }
                    } else {
                        let mut lock = get_app().group_bot.write().await;
                        lock.insert(group_id, bot_id);
                    }

                    let s = e.inner.elements.to_string();
                    match &*s {
                        "萝卜子列表" => {
                            let app = get_app();
                            let bots = app.bots().read().await;

                            let mut s = String::from("在线的萝卜子\n");
                            for bot in bots.iter() {
                                s.push_str(format!("{0}: {1}", bot.client().account_info.read().await.nickname, bot.client().uin().await).as_str());
                                s.push('\n');
                            }
                            s.pop();

                            let chain = MessageChain::new(Text::new(s));

                            e.client.send_group_message(e.inner.group_code, chain).await.expect("Error");
                        }
                        _ => {}
                    };
                }
                _ => {}
            }
        });
    }
}
