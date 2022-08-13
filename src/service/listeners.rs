use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex, RwLock};

use crate::{Event, Listener};

type LimitedListeners = LinkedList<Arc<RwLock<Option<Arc<Listener>>>>>;

pub struct ListenerWorker {
    listeners: Vec<LimitedListeners>,
    listener_rx: Mutex<tokio::sync::mpsc::Receiver<Arc<Listener>>>,
    listener_tx: tokio::sync::mpsc::Sender<Arc<Listener>>,
    closed: AtomicBool,
}

impl ListenerWorker {
    pub fn new() -> Self {
        let mut listeners = vec![];

        for _ in 0..5 {
            listeners.push(LinkedList::new())
        }

        let (tx, rx) = tokio::sync::mpsc::channel(10);

        ListenerWorker {
            listeners,
            listener_rx: rx.into(),
            listener_tx: tx,
            closed: AtomicBool::new(false),
        }
    }

    pub async fn schedule(&self, listener: Listener) {
        let arc = Arc::new(listener);
        let _ = self.listener_tx.send(arc).await;
    }

    pub async fn handle(&self, event: &Event) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        let mut handlers = vec![];
        for list in &self.listeners {
            handlers.reserve(list.len());
            for opt in list.into_iter().cloned() {
                let event = event.clone();
                let handle = tokio::spawn(async move {
                    let listener = {
                        let lock = opt.read().await;
                        lock.as_ref().map(|a| Arc::clone(a))
                    };

                    if let Some(listener) = listener {
                        let close_listener = async {
                            let mut lock = opt.write().await;
                            *lock = None;
                        };

                        if let Some(ref mutex) = listener.concurrent_mutex {
                            mutex.lock().await;
                        }

                        if listener.closed.load(Ordering::Acquire) {
                            close_listener.await;
                            return;
                        }

                        let fu = (listener.handler)(event);
                        let con: bool = fu.await;

                        if !con {
                            close_listener.await;
                        };
                    }
                });

                handlers.push(handle);
            }

            //waiting for all task finish
            for _ in 0..handlers.len() {
                let handle = handlers.pop().expect("Cannot get handle");
                let _ = handle.await;
            }

            if event.is_intercepted() {
                break;
            }
        }
    }

    pub async fn start(&self) {
        let mut lock = self
            .listener_rx
            .try_lock()
            .unwrap_or_else(|_| panic!("ListenerWorker只可开启一次"));

        'add: while let Some(l) = lock.recv().await {
            let list = &self.listeners[l.priority as usize];

            for opt in list {
                let node = { opt.read().await.clone() }; //限制生命周期

                if node.is_none() {
                    let mut wl = opt.write().await;
                    *wl = Some(l);
                    continue 'add;
                }
            }

            // Safety: locked by the mpsc::channel,
            // and the node will not remove when listener close
            #[allow(clippy::cast_ref_to_mut)]
            let list = unsafe { &mut *(list as *const _ as *mut LimitedListeners) };
            list.push_back(Arc::new(RwLock::new(Some(l))));
        }
    }

    pub fn close(&self) {
        self.closed.swap(true, Ordering::Release);
    }
}

pub fn get_global_worker() -> &'static ListenerWorker {
    static WORKER: OnceLock<ListenerWorker> = OnceLock::new();
    WORKER.get_or_init(ListenerWorker::new)
}
