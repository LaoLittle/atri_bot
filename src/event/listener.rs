use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::channel::global_receiver;
use crate::event::FromEvent;
use crate::{global_listener_runtime, global_listener_worker, Event};

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
    pub fn listening_on<E, F, Fu>(handler: F) -> ListenerGuard
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = bool>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        ListenerBuilder::listening_on(handler).start()
    }

    pub fn listening_on_always<E, F, Fu>(handler: F) -> ListenerGuard
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = ()>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        ListenerBuilder::listening_on_always(handler).start()
    }

    pub async fn next_event_with_priority<E, F>(
        timeout: Duration,
        filter: F,
        priority: Priority,
    ) -> Option<E>
    where
        E: FromEvent,
        E: Send + 'static,
        F: Fn(&E) -> bool,
    {
        tokio::time::timeout(timeout, async {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let _guard = ListenerBuilder::listening_on_always(move |e: E| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(e).await;
                }
            })
            .priority(priority)
            .start();

            while let Some(e) = rx.recv().await {
                if !filter(&e) {
                    continue;
                }
                drop(_guard);

                return e;
            }

            unreachable!()
        })
        .await
        .ok()
    }

    #[inline]
    pub async fn next_event<E, F>(timeout: Duration, filter: F) -> Option<E>
    where
        E: FromEvent,
        E: Send + 'static,
        F: Fn(&E) -> bool,
    {
        Self::next_event_with_priority(timeout, filter, Default::default()).await
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct ListenerBuilder {
    pub name: Option<String>,
    pub concurrent: bool,
    pub watcher: bool,
    handler: ListenerHandler,
    pub priority: Priority,
}

impl ListenerBuilder {
    fn new<F, Fu>(handler: F) -> Self
    where
        F: Fn(Event) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = bool>,
        Fu: Send + 'static,
    {
        let handler = Box::new(move |e| {
            let fu = handler(e);
            let b: Box<dyn Future<Output = bool> + Send + 'static> = Box::new(fu);

            Box::into_pin(b)
        });

        Self {
            name: None,
            concurrent: true,
            watcher: false,
            handler,
            priority: Priority::Middle,
        }
    }

    fn new_always<F, Fu>(handler: F) -> Self
    where
        F: Fn(Event) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = ()>,
        Fu: Send + 'static,
    {
        Self::new(move |e| {
            let fu = handler(e);

            async move {
                fu.await;
                true
            }
        })
    }

    pub fn listening_on<E, F, Fu>(handler: F) -> Self
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = bool>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        Self::new(move |e| {
            let fu = E::from_event(e).map(&handler);

            async move {
                if let Some(fu) = fu {
                    fu.await
                } else {
                    true
                }
            }
        })
    }

    pub fn listening_on_always<E, F, Fu>(handler: F) -> Self
    where
        F: Fn(E) -> Fu,
        F: Send + 'static,
        Fu: Future<Output = ()>,
        Fu: Send + 'static,
        E: FromEvent,
    {
        Self::new_always(move |e| {
            let fu = E::from_event(e).map(&handler);

            async move {
                if let Some(fu) = fu {
                    fu.await;
                }
            }
        })
    }

    pub fn start(self) -> ListenerGuard {
        let Self {
            name,
            concurrent,
            watcher,
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

        if watcher {
            global_listener_runtime().spawn(async move {
                while let Ok(event) = global_receiver().recv().await {
                    if let Some(ref mutex) = listener.concurrent_mutex {
                        let _ = mutex.lock().await;
                    }

                    let fu = (listener.handler)(event);

                    let keep: bool = fu.await;
                    if (!keep) || listener.closed.load(Ordering::Relaxed) {
                        break;
                    };
                }
            });
        } else {
            global_listener_runtime().spawn(global_listener_worker().schedule(listener));
        }

        ListenerGuard {
            name: arc_name,
            closed: arc_closed,
        }
    }

    #[inline]
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    #[inline]
    pub fn concurrent(mut self, is: bool) -> Self {
        self.concurrent = is;
        self
    }

    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn watcher(mut self, is: bool) -> Self {
        self.watcher = is;
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

impl From<u8> for Priority {
    #[inline]
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Top,
            1 => Self::High,
            2 => Self::Middle,
            3 => Self::Low,
            4 => Self::Base,
            _ => Self::Middle,
        }
    }
}

unsafe impl Send for Listener {}

unsafe impl Sync for Listener {}

#[must_use = "if unused the Listener will immediately close"]
pub struct ListenerGuard {
    name: Arc<String>,
    closed: Arc<AtomicBool>,
}

impl ListenerGuard {
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl Drop for ListenerGuard {
    #[inline]
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}
