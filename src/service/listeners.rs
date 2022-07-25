use std::collections::LinkedList;
use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex, RwLock};

use crate::{Event, Listener};

type LimitedListeners = LinkedList<Arc<RwLock<Option<Arc<Listener>>>>>;

pub struct ListenerWorker {
    listeners: Vec<LimitedListeners>,
    listener_rx: Mutex<tokio::sync::mpsc::Receiver<Arc<Listener>>>,
    listener_tx: tokio::sync::mpsc::Sender<Arc<Listener>>,
}

impl ListenerWorker {
    pub async fn schedule(&self, listener: Listener) {
        let arc = Arc::new(listener);
        let _ = self.listener_tx.send(arc).await;
    }

    pub async fn handle(&self, event: &Event) {
        let mut handlers = vec![];
        for list in &self.listeners {
            handlers.reserve(list.len());
            for opt in list {
                let opt = opt.clone();
                let event = event.clone();
                let handle = tokio::spawn(async move {
                    let listener = {
                        let lock = opt.read().await;
                        lock.clone()
                    };

                    if let Some(listener) = listener {
                        if let Some(mutex) = listener.concurrent_mutex.clone() {
                            mutex.lock().await;
                        }

                        let fu = (listener.handler)(event);
                        let con: bool = fu.await;

                        if !con {
                            let mut lock = opt.write().await;
                            *lock = None;
                        };
                    }
                });

                handlers.push(handle);
            }

            for _ in 0..handlers.len() {
                let handle = handlers.pop().expect("Cannot get handle");
                let _ = handle.await;
            }

            if event.is_intercepted() { break; }
        }
    }

    pub async fn start(&self) {
        let mut lock = self.listener_rx.lock().await;

        'add:
        while let Some(l) = lock.recv().await {
            let list = &self.listeners[l.priority as usize];

            for opt in list {
                let node = { opt.read().await.clone() };

                if let None = node {
                    let mut wl = opt.write().await;
                    *wl = Some(l);
                    continue 'add;
                }
            }

            let list = unsafe { &mut *(list as *const _ as *mut LimitedListeners) };
            list.push_back(Arc::new(RwLock::new(Some(l))));
        }
    }
}

pub fn get_global_worker() -> &'static ListenerWorker {
    static WORKER: OnceLock<ListenerWorker> = OnceLock::new();
    WORKER.get_or_init(|| {
        let mut listeners = vec![];

        for _ in 0..5 {
            listeners.push(LinkedList::new())
        }

        let (tx, rx) = tokio::sync::mpsc::channel(61);

        ListenerWorker {
            listeners,
            listener_rx: rx.into(),
            listener_tx: tx,
        }
    })
}