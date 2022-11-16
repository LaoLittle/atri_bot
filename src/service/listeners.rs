use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::{Event, Listener};

type LimitedListeners = LinkedList<Arc<RwLock<Option<Arc<Listener>>>>>;

pub struct ListenerWorker {
    listeners: [LimitedListeners; 5],
    listener_rx: Mutex<tokio::sync::mpsc::Receiver<Arc<Listener>>>,
    listener_tx: tokio::sync::mpsc::Sender<Arc<Listener>>,
    closed: AtomicBool,
    runtime: tokio::runtime::Runtime,
}

impl ListenerWorker {
    #[inline]
    pub fn new() -> Self {
        Self::new_with_runtime(tokio::runtime::Runtime::new().unwrap())
    }

    pub fn new_with_runtime(runtime: tokio::runtime::Runtime) -> Self {
        let listeners = [
            LinkedList::new(),
            LinkedList::new(),
            LinkedList::new(),
            LinkedList::new(),
            LinkedList::new(),
        ];

        let (tx, rx) = tokio::sync::mpsc::channel(10);

        ListenerWorker {
            listeners,
            listener_rx: rx.into(),
            listener_tx: tx,
            closed: AtomicBool::new(false),
            runtime,
        }
    }

    pub fn runtime(&self) -> &tokio::runtime::Runtime {
        &self.runtime
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub async fn schedule(&self, listener: Listener) {
        let arc = Arc::new(listener);
        let _ = self.listener_tx.send(arc).await;
    }

    pub async fn handle(&self, event: &Event) {
        if self.closed() {
            return;
        }

        let mut handles = vec![];
        for list in &self.listeners {
            handles.reserve(list.len());
            for opt in list.iter().map(Arc::clone) {
                let event = event.clone();
                let handle = tokio::spawn(async move {
                    let listener = {
                        let lock = opt.read().await;
                        lock.as_ref().map(Arc::clone)
                    };

                    if let Some(listener) = listener {
                        let close_listener = async {
                            let mut lock = opt.write().await;
                            *lock = None;
                        };

                        if let Some(ref mutex) = listener.concurrent_mutex {
                            let _ = mutex.lock().await;
                        }

                        if listener.closed.load(Ordering::Relaxed) {
                            close_listener.await;
                            return;
                        }

                        let fu = (listener.handler)(event);

                        let keep: bool = fu.await;
                        if !keep {
                            close_listener.await;
                        };
                    }
                });

                handles.push(handle);
            }

            while let Some(handle) = handles.pop() {
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
        self.closed.swap(true, Ordering::Relaxed);
    }
}

impl Default for ListenerWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ListenerWorker {
    fn drop(&mut self) {
        self.close();
    }
}
