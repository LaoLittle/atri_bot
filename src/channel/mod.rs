use std::panic::catch_unwind;
use std::sync::OnceLock;
use std::thread;

use async_trait::async_trait;
use ricq::handler::QEvent;
use tokio::sync::broadcast::{channel, Receiver, Sender};

static GLOBAL_EVENT_CHANNEL: OnceLock<Sender<QEvent>> = OnceLock::<Sender<QEvent>>::new();

pub fn global_sender() -> &'static Sender<QEvent> {
    GLOBAL_EVENT_CHANNEL.get_or_init(|| {
        let channel = channel(128);

        channel.0
    })
}

pub fn global_receiver() -> Receiver<QEvent> {
    global_sender().subscribe()
}

pub struct GlobalEventBroadcastHandler;

#[async_trait]
impl ricq::handler::Handler for GlobalEventBroadcastHandler {
    async fn handle(&self, event: QEvent) {
        /*let event = match event {
            QEvent::GroupMessage(e) => {


                Event::GroupMessage(e.into())
            }
            QEvent::Login(id) => {
                Event::Other(event)
            }
            e => {
                Event::Other(e)
            }
        };*/

        match event {
            QEvent::GroupMessage(ref e) => {
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            QEvent::FriendMessage(ref e) => {
                if e.client.uin().await == e.inner.from_uin { return; }
            }
            _ => {}
        }

        let _ = global_sender().send(event);
    }
}