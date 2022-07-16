use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use regex::Regex;
use ricq::handler::QEvent;
use ricq::msg::elem::Text;
use ricq::msg::MessageChain;
use skia_safe::EncodedImageFormat;
use tracing::error;

use crate::{check_group, unwrap_result_or_print_err_return};
use crate::channel::global_receiver;
use crate::fun::drawmeme::get_image_or_wait;
use crate::fun::drawmeme::zero::zero;

pub mod drawmeme;
pub mod game;

pub async fn handler() {
    let mut rx = global_receiver();

    let zero_reg = Regex::new("^#(\\d{1,3})").expect("Unknown regex");
    let zero_reg = Arc::new(zero_reg);
    while let Ok(e) = rx.recv().await {
        let zero_reg = zero_reg.clone();
        tokio::spawn(async move {
            match e {
                QEvent::GroupMessage(e) => {
                    check_group!(e);

                    let bot_id = e.client.uin().await;
                    let group_id = e.inner.group_code;

                    let msg = e.inner.elements.clone();
                    let s = msg.to_string();
                    let find = zero_reg.captures(&s);

                    if let Some(cap) = find {
                        let num = unwrap_result_or_print_err_return!(u8::from_str(&cap[1]));
                        if num > 100 { return; }

                        let mut img = None::<Bytes>;
                        if let Err(_) = get_image_or_wait(&e, &mut img).await {
                            let mut req = MessageChain::default();
                            req.push(Text::new("超时未发送".into()));
                            if let Some(reply) = e.inner.elements.reply() {

                                req.with_reply(reply);
                            }
                            e.client.send_group_message(group_id, req).await.ok();
                            return;
                        };

                        let zero = if let Some(img) = zero(num, &img.expect("Cannot be none")) {
                            img
                        } else { return; };

                        let mut chain = MessageChain::default();
                        let vec: Vec<u8> = zero.encode_to_data(EncodedImageFormat::PNG).expect("Cannot encode image").to_vec();
                        let image = unwrap_result_or_print_err_return!(e.client.upload_group_image(group_id, vec).await);
                        chain.push(image);
                        if let Err(err) = e.client.send_group_message(e.inner.group_code, chain).await {
                            error!(
                                "Bot({})发送信息失败, 目标群: {}({}), {:?}",
                                bot_id,
                                e.inner.group_name,
                                group_id,
                                err
                            )
                        };

                        return;
                    }
                }
                _ => {}
            }
        });
    }
}