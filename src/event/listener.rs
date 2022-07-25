use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};

use crate::{Event, get_listener_runtime};
use crate::event::FromEvent;
use crate::service::listeners::get_global_worker;

pub struct Listener {
    pub(crate) name: Option<String>,
    pub(crate) concurrent_mutex: Option<Arc<Mutex<()>>>,
    pub(crate) handler: Box<dyn Fn(Event) -> Pin<Box<dyn Future<Output=bool> + Send + 'static>> + Send + 'static>,
    pub(crate) priority: Priority,
}

#[derive(Copy, Clone)]
pub enum Priority {
    Top = 0,
    High = 1,
    Middle = 2,
    Low = 3,
    Base = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Middle
    }
}

unsafe impl Send for Listener {}

unsafe impl Sync for Listener {}

impl Listener {
    pub fn new<F, Fu>(handler: F) -> Self
        where F: Fn(Event) -> Fu,
              F: Send + 'static,
              Fu: Future<Output=bool>,
              Fu: Send + 'static
    {
        let handler = Box::new(move |e: Event| {
            let fu = handler(e);
            let b: Box<dyn Future<Output=bool> + Send + 'static> = Box::new(fu);
            Box::into_pin(b)
        });

        Listener {
            name: None,
            concurrent_mutex: None,
            handler,
            priority: Priority::Middle,
        }
    }

    pub fn new_always<F, Fu>(handler: F) -> Self
        where F: Fn(Event) -> Fu,
              F: Send + 'static,
              Fu: Future<Output=()>,
              Fu: Send + 'static
    {
        Self::new(
            move |e: Event| {
                let fu = handler(e);
                let b: Box<dyn Future<Output=bool> + Send + 'static> = Box::new(async move {
                    fu.await;
                    true
                });
                Box::into_pin(b)
            }
        )
    }

    pub fn listening_on<E, F, Fu>(handler: F) -> Self
        where F: Fn(E) -> Fu,
              F: Send + 'static,
              Fu: Future<Output=bool>,
              Fu: Send + 'static,
              E: FromEvent
    {
        async fn t() -> bool {
            true
        }

        Self::new(
            move |e: Event| {
                let b: Box<dyn Future<Output=bool> + Send + 'static> = if let Some(e) = E::from_event(e) {
                    let fu = handler(e);
                    Box::new(fu)
                } else { Box::new(t()) };

                Box::into_pin(b)
            }
        )
    }

    pub fn listening_on_always<E, F, Fu>(handler: F) -> Self
        where F: Fn(E) -> Fu,
              F: Send + 'static,
              Fu: Future<Output=()>,
              Fu: Send + 'static,
              E: FromEvent
    {
        async fn t() -> bool {
            true
        }

        Self::new(
            move |e: Event| {
                let b: Box<dyn Future<Output=bool> + Send + 'static> = if let Some(e) = E::from_event(e) {
                    let fu = handler(e);
                    Box::new(async move {
                        fu.await;
                        true
                    })
                } else { Box::new(t()) };

                Box::into_pin(b)
            }
        )
    }

    pub fn with_name(mut self, name: impl ToString) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn synchronize(mut self) -> Self {
        self.concurrent_mutex = None;
        self
    }

    pub fn concurrent(mut self) -> Self {
        self.concurrent_mutex = Some(Mutex::new(()).into());
        self
    }

    pub fn set_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn start(self) -> ListenerGuard {
        let (sigtx, mut sigrx) = oneshot::channel::<()>();

        get_listener_runtime().spawn(async move {
            get_global_worker().schedule(self).await;
        });

        ListenerGuard {
            signal_tx: sigtx,
        }
    }
}

pub struct ListenerGuard {
    signal_tx: oneshot::Sender<()>,
}

impl ListenerGuard {
    pub async fn complete(self) {
        let _ = self.signal_tx.send(());
    }
}