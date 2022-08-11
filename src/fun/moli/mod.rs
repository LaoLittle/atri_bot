mod data;

use dashmap::DashSet;
use rand::Rng;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::event::listener::ListenerGuard;
use crate::event::GroupMessageEvent;
use crate::{get_app, Listener};
use ricq::msg::elem::{RQElem, Reply};
use ricq::msg::MessageChainBuilder;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::fun::moli::data::{MoliMessage, MoliResponse};
use crate::service::Service;

static MOLI_REQ_URL: &str = "https://api.mlyai.com/reply";
pub fn moli_listener() -> ListenerGuard {
    let mut serivce = Service::new("moli");
    let mut path = PathBuf::new();
    path.push("config");
    serivce.with_path(path);

    let config: MoliConfig = serivce.read_config();
    let cfg = Arc::new(config);

    let set = DashSet::<i64>::new();
    let set = Arc::new(set);

    Listener::listening_on_always(move |e: GroupMessageEvent| {
        let config = cfg.clone();

        let set = set.clone();
        async move {
            if set.contains(&e.message().from_uin) {
                return;
            }
            let f = || {
                let msg = e.message().clone();
                for elem in msg.elements {
                    match elem {
                        RQElem::At(at) if at.target == e.bot().id() => return true,
                        RQElem::Text(s) if s.content.contains(&config.name) => return true,
                        _ => {}
                    }
                }
                false
            };

            if !f() {
                return;
            }

            async fn handle_message(
                e: &GroupMessageEvent,
                config: &MoliConfig,
            ) -> Result<(), Box<dyn Error>> {
                let msg = MoliMessage::from_group_message(
                    e.message().clone(),
                    e.group()
                        .find_member(e.message().from_uin)
                        .await
                        .expect("Cannot find member")
                        .card_name()
                        .into(),
                );

                let json = serde_json::to_string(&msg)?;
                let resp = get_app()
                    .http_client()
                    .post(MOLI_REQ_URL)
                    .header("Api-Key", &config.api_key)
                    .header("Api-Secret", &config.api_secret)
                    .header("Content-Type", "application/json;charset=UTF-8")
                    .body(json)
                    .send()
                    .await?;

                let resp: MoliResponse = serde_json::from_slice(&resp.bytes().await?)?;

                if config.do_print_results_on_console {
                    info!("Molly: 服务器返回数据: {:?}", resp);
                }

                if resp.code != "00000" {
                    error!(
                        "Molly: 出现异常: code={} message={}",
                        resp.code, resp.message
                    );
                    return Ok(());
                }

                let mut msg = MessageChainBuilder::new();

                for dat in resp.data {
                    match dat.typed {
                        1 => {
                            msg.push_str(&dat.content);
                        }
                        2 => {
                            let img = String::from("https://files.molicloud.com/") + &dat.content;
                            let img = get_app().http_client().get(img).send().await?;

                            msg.push(e.group().upload_image(img.bytes().await?.to_vec()).await?);
                        }
                        _ => {}
                    };
                }

                if config.do_quote_reply {
                    let r = Reply {
                        reply_seq: e.message().seqs[0],
                        sender: e.message().from_uin,
                        time: e.message().time,
                        elements: e.message().elements.clone(),
                    };

                    msg.push(r);
                }

                e.group().send_message(msg.build()).await?;

                Ok(())
            }

            let sender = e.message().from_uin;
            set.insert(sender);

            let mut e = e;

            if let Err(e) = handle_message(&e, &config).await {
                error!("Moli: Error on handle message {}", e);
            }

            for _ in 0..config.reply_times {
                e = if let Ok(e) = e
                    .next_event(Duration::from_secs(10), |e| e.message().from_uin == sender)
                    .await
                {
                    e
                } else {
                    let mut msg = MessageChainBuilder::new();
                    let random = rand::thread_rng().gen_range(0..config.timeout_reply.len());
                    msg.push_str(&config.timeout_reply[random]);
                    let _ = e.group().send_message(msg.build()).await;

                    set.remove(&sender);
                    return;
                };

                if let Err(e) = handle_message(&e, &config).await {
                    error!("Moli: Error on handle message {}", e);
                }
            }
            set.remove(&sender);
        }
    })
    .with_name("Moli-Chat")
    .start()
}

#[derive(Serialize, Deserialize)]
struct MoliConfig {
    api_key: String,
    api_secret: String,
    name: String,
    reply_times: u8,
    do_quote_reply: bool,
    do_print_results_on_console: bool,
    default_reply: Vec<String>,
    timeout_reply: Vec<String>,
}

impl Default for MoliConfig {
    fn default() -> Self {
        Self {
            api_key: Default::default(),
            api_secret: Default::default(),
            name: String::from("亚托莉"),
            reply_times: 0,
            do_quote_reply: false,
            do_print_results_on_console: false,
            default_reply: vec![
                "？".into(),
                "怎么".into(),
                "怎么了".into(),
                "什么？".into(),
                "在".into(),
                "嗯？".into(),
            ],
            timeout_reply: vec![
                "没事我就溜了".into(),
                "emmmmm".into(),
                "......".into(),
                "溜了".into(),
                "？".into(),
            ],
        }
    }
}
