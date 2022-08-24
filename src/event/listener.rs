use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::event::FromEvent;
use crate::service::listeners::get_global_worker;
use crate::{get_listener_runtime, Event};

pub type ListenerHandler =
    Box<dyn Fn(Event) -> Pin<Box<dyn Future<Output = bool> + Send + 'static>> + Send + 'static>;

pub struct Listener {
    pub name: Arc<String>,
    pub(crate) concurrent_mutex: Option<Mutex<()>>,
    pub(crate) handler: ListenerHandler,
    pub(crate) closed: Arc<AtomicBool>,
    pub(crate) priority: Priority,
}

impl Listener {
    fn new<F, Fu>(handler: F) -> ListenerBuilder
    where
        F: Fn(Event) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = bool>,
        Fu: Send + 'static,
    {
        let handler = Box::new(move |e: Event| {
            let fu = handler(e);
            let b: Box<dyn Future<Output = bool> + Send + 'static> = Box::new(fu);

            Box::into_pin(b)
        });

        ListenerBuilder {
            name: None,
            concurrent: true,
            handler,
            priority: Priority::Middle,
        }
    }

    fn new_always<F, Fu>(handler: F) -> ListenerBuilder
    where
        F: Fn(Event) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = ()>,
        Fu: Send + 'static,
    {
        Self::new(move |e: Event| {
            let fu = handler(e);
            let b: Box<dyn Future<Output = bool> + Send + 'static> = Box::new(async move {
                fu.await;
                true
            });

            Box::into_pin(b)
        })
    }

    pub fn listening_on<E, F, Fu>(handler: F) -> ListenerBuilder
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = bool>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        Self::new(move |e: Event| {
            let b: Box<dyn Future<Output = bool> + Send + 'static> =
                if let Some(e) = E::from_event(e) {
                    let fu = handler(e);
                    Box::new(fu)
                } else {
                    Box::new(bool_true())
                };

            Box::into_pin(b)
        })
    }

    pub fn listening_on_always<E, F, Fu>(handler: F) -> ListenerBuilder
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = ()>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        Self::new_always(move |e: Event| {
            let b: Box<dyn Future<Output = ()> + Send + 'static> = if let Some(e) = E::from_event(e)
            {
                let fu = handler(e);
                Box::new(async move {
                    fu.await;
                })
            } else {
                Box::new(nop())
            };

            Box::into_pin(b)
        })
    }

    pub async fn next_event<E, F>(timeout: Duration, filter: F) -> Option<E>
    where
        E: FromEvent,
        E: Send + 'static,
        F: Fn(&E) -> bool,
    {
        tokio::time::timeout(timeout, async {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let _guard = Listener::listening_on_always(move |e: E| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(e).await;
                }
            })
            .start();

            while let Some(e) = rx.recv().await {
                if !filter(&e) {
                    continue;
                }

                return e;
            }

            unreachable!()
        })
        .await
        .ok()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct ListenerBuilder {
    pub name: Option<String>,
    pub concurrent: bool,
    handler: ListenerHandler,
    pub priority: Priority,
}

impl ListenerBuilder {
    pub fn start(self) -> ListenerGuard {
        let Self {
            name,
            concurrent,
            handler,
            priority,
        } = self;

        let name = Arc::new(name.unwrap_or_else(|| String::from("Unnamed-Listener")));
        let arc_name = name.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let arc_closed = closed.clone();
        let listener = Listener {
            name,
            concurrent_mutex: if concurrent {
                None
            } else {
                Some(Mutex::new(()))
            },
            handler,
            closed,
            priority,
        };

        get_listener_runtime().spawn(get_global_worker().schedule(listener));

        ListenerGuard {
            name: arc_name,
            closed: arc_closed,
        }
    }

    pub fn with_name(mut self, name: impl ToString) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn synchronize(mut self) -> Self {
        self.concurrent = false;
        self
    }

    pub fn concurrent(mut self) -> Self {
        self.concurrent = false;
        self
    }

    pub fn set_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

#[derive(Copy, Clone, Default)]
pub enum Priority {
    Top = 0,
    High = 1,
    #[default]
    Middle = 2,
    Low = 3,
    Base = 4,
}

unsafe impl Send for Listener {}

unsafe impl Sync for Listener {}

pub struct ListenerGuard {
    name: Arc<String>,
    closed: Arc<AtomicBool>,
}

impl ListenerGuard {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.closed.swap(true, Ordering::Relaxed);
    }
}

async fn bool_true() -> bool {
    true
}

async fn nop() {}
