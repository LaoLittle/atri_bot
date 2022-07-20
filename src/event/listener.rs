use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, trace};

use crate::{Event, get_listener_runtime, global_receiver};

pub struct Listener {
    name: Option<String>,
    concurrent: bool,
    handler: Box<dyn Fn(Event) -> Pin<Box<dyn Future<Output=bool> + Send + 'static>> + Send + 'static>,
}

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
            concurrent: true,
            handler,
        }
    }

    pub fn new_always<F, Fu>(handler: F) -> Self
        where F: Fn(Event) -> Fu,
              F: Send + 'static,
              Fu: Future<Output=()>,
              Fu: Send + 'static
    {
        let handler = Box::new(move |e: Event| {
            let fu = handler(e);
            let b: Box<dyn Future<Output=bool> + Send + 'static> = Box::new(async move {
                fu.await;
                true
            });
            Box::into_pin(b)
        });

        Listener {
            name: None,
            concurrent: true,
            handler,
        }
    }

    pub fn name(mut self, name: impl ToString) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn synchronize(mut self) -> Self {
        self.concurrent = false;
        self
    }

    pub fn concurrent(mut self) -> Self {
        self.concurrent = true;
        self
    }

    pub fn finish(self) -> ListenerGuard {
        let (sigtx, mut sigrx) = oneshot::channel::<()>();
        let handle = get_listener_runtime().spawn(async move {
            let mut rx = global_receiver();

            if self.concurrent {
                let finished = Arc::new(AtomicBool::new(false));
                let mut handles = vec![];
                while let Ok(e) = rx.recv().await {
                    if let Ok(()) | Err(oneshot::error::TryRecvError::Closed) = sigrx.try_recv() {
                        debug!("Listener Closing...");
                        break;
                    }

                    if finished.load(Ordering::Relaxed) { break; }

                    let fu = (self.handler)(e);
                    let finished = finished.clone();
                    handles.push(
                        tokio::spawn(async move {
                            let f: bool = fu.await;
                            if !f { finished.swap(true, Ordering::Release); }
                        })
                    );
                }

                for handle in handles {
                    let _ = timeout(Duration::from_secs(300), handle).await;
                }
            } else {
                while let Ok(e) = rx.recv().await {
                    if let Ok(()) | Err(oneshot::error::TryRecvError::Closed) = sigrx.try_recv() {
                        break;
                    }

                    let fu = (self.handler)(e);

                    let finished: bool = fu.await;
                    if !finished { break; }
                }
            }
        });

        ListenerGuard {
            signal_tx: sigtx,
            handle,
        }
    }
}

pub struct ListenerGuard {
    signal_tx: oneshot::Sender<()>,
    handle: JoinHandle<()>,
}

impl ListenerGuard {
    pub async fn complete(self) {
        let _ = self.signal_tx.send(());
        let _ = self.handle.await;
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}