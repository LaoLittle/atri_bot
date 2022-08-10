use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use regex::Regex;
use ricq::msg::elem::{RQElem, Reply, Text};
use ricq::msg::{MessageChain, MessageChainBuilder};
use skia_safe::{Bitmap, EncodedImageFormat};
use tokio::time::error::Elapsed;

use crate::event::listener::ListenerGuard;
use crate::event::GroupMessageEvent;
use crate::fun::drawmeme::zero::zero;
use crate::{get_app, unwrap_result_or_print_err_return, Listener};

pub mod zero;

pub trait Meme {
    fn draw(args: &[MemeArg]) -> Bitmap;
}

pub enum MemeArg {
    Text(String),
    Image(Bytes),
}

pub enum MemeError {
    Other(String),
}

pub fn drawmeme_listener() -> ListenerGuard {
    let zero_reg = Regex::new("^#(\\d{1,3})").expect("Unknown regex");
    let zero_reg = Arc::new(zero_reg);

    let z = zero_reg.clone();

    Listener::listening_on_always(move |e: GroupMessageEvent| {
        let zero_reg = z.clone();
        async move {
            let bot = e.bot().clone();
            let group_id = e.group().id();

            let msg = e.message().elements.clone();
            let s = msg.to_string();
            let find = zero_reg.captures(&s);

            if let Some(cap) = find {
                let num = unwrap_result_or_print_err_return!(u8::from_str(&cap[1]));
                if num > 100 {
                    return;
                }

                let mut img = None::<Bytes>;
                if get_image_or_wait(&e, &mut img).await.is_err() {
                    return;
                };

                let zero = if let Some(img) = zero(num, &img.expect("Cannot be none")) {
                    img
                } else {
                    return;
                };

                let mut chain = MessageChain::default();
                let vec: Vec<u8> = zero
                    .encode_to_data(EncodedImageFormat::PNG)
                    .expect("Cannot encode image")
                    .to_vec();
                let image = unwrap_result_or_print_err_return!(e.group().upload_image(vec).await);
                chain.push(image);
                let _ = e.group().send_message(chain).await;
            }
        }
    })
    .with_name("DrawMeme")
    .start()
}

pub async fn get_image_or_wait(
    event: &GroupMessageEvent,
    img: &mut Option<Bytes>,
) -> Result<(), Elapsed> {
    let msg = event.message().elements.clone();
    async fn get_img(msg: MessageChain, img: &mut Option<Bytes>) {
        for elem in msg {
            if let RQElem::GroupImage(i) = elem {
                let resp = unwrap_result_or_print_err_return!(
                    get_app().http_client().get(i.url()).send().await
                );

                *img = Some(unwrap_result_or_print_err_return!(resp.bytes().await));
                break;
            }
        }
    }

    get_img(msg, img).await;

    if img.is_none() {
        let mut req = MessageChain::default();
        req.push(Text::new("请在30秒内发送图片".into()));
        event.group().send_message(req).await.ok();
    }

    let m = match event
        .next_message(Duration::from_secs(30), |m| {
            let m = m.clone();

            for elem in m {
                if let RQElem::GroupImage(..) = elem {
                    return true;
                }
            }
            false
        })
        .await
    {
        Ok(m) => m,
        Err(e) => {
            let mut req = MessageChainBuilder::new();
            req.push_str("超时未发送");

            let reply = Reply {
                time: event.message().time,
                reply_seq: event.message().seqs[0],
                sender: event.message().from_uin,
                elements: event.message().elements.clone(),
            };

            req.push(reply);

            event.group().send_message(req.build()).await.ok();
            return Err(e);
        }
    };

    get_img(m, img).await;

    Ok(())
}
