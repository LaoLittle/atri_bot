use std::mem;
use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use regex::Regex;
use ricq::msg::elem::{Reply, Text};
use ricq::msg::MessageChain;
use skia_safe::EncodedImageFormat;
use tracing::error;

use crate::{Event, unwrap_result_or_print_err_return};
use crate::event::listener::Listener;
use crate::fun::drawmeme::get_image_or_wait;
use crate::fun::drawmeme::zero::zero;

pub mod drawmeme;
pub mod game;

pub async fn handler() {
    let zero_reg = Regex::new("^#(\\d{1,3})").expect("Unknown regex");
    let zero_reg = Arc::new(zero_reg);

    let z = zero_reg.clone();

    let guard = Listener::new_always(move |e| {
        let zero_reg = z.clone();
        async move {
            match e {
                Event::GroupMessageEvent(e) => {
                    let bot = e.group().bot().clone();
                    let group_id = e.group().id();

                    let msg = e.message().elements.clone();
                    let s = msg.to_string();
                    let find = zero_reg.captures(&s);

                    if let Some(cap) = find {
                        let num = unwrap_result_or_print_err_return!(u8::from_str(&cap[1]));
                        if num > 100 { return; }

                        let mut img = None::<Bytes>;
                        if let Err(_) = get_image_or_wait(&e, &mut img).await {
                            let mut req = MessageChain::default();
                            req.push(Text::new("超时未发送".into()));

                            let mut reply = Reply::default();

                            reply.time = e.message().time;
                            reply.reply_seq = e.message().seqs[0];
                            reply.sender = e.message().from_uin;
                            reply.elements = msg;
                            req.with_reply(reply);

                            e.group().send_message(req).await.ok();
                            return;
                        };

                        let zero = if let Some(img) = zero(num, &img.expect("Cannot be none")) {
                            img
                        } else { return; };

                        let mut chain = MessageChain::default();
                        let vec: Vec<u8> = zero.encode_to_data(EncodedImageFormat::PNG).expect("Cannot encode image").to_vec();
                        let image = unwrap_result_or_print_err_return!(e.group().upload_image(vec).await);
                        chain.push(image);
                        if let Err(err) = e.group().send_message(chain).await {
                            error!(
                                "{}发送信息失败, 目标群: {}({}), {:?}",
                                bot,
                                e.group().name(),
                                group_id,
                                err
                            )
                        };
                    }
                }
                _ => {}
            }
        }
    })
        .with_name("Fun")
        .start();

    mem::forget(guard);
}