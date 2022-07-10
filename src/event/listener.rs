use std::mem;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use regex::{Match, Regex};
use ricq::client::event::GroupMessageEvent;
use ricq::handler::QEvent;

use crate::channel::global_receiver;
use crate::event::Event;
/*
#[derive(Default)]
pub struct GroupMessageListener {
    finding: Vec<Finding>,
}

unsafe impl Send for GroupMessageListener {}

struct Finding {
    invoke: Box<dyn Fn(Match<'static>, GroupMessageEvent) -> Pin<Box<dyn Future<Output=bool> + Send + 'static>> + Send + 'static>,
    regex: Arc<Regex>,
}

impl GroupMessageListener {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finding<O, F>(&mut self, r: &Arc<Regex>, op: O)
        where O: Fn(Match<'static>, GroupMessageEvent) -> F,
              O: Send + 'static,
              F: Future<Output=bool> + Send + 'static
    {
        let r0 = r.clone();

        let new_fn = Box::new(move |m: Match<'static>, e: GroupMessageEvent| {
            Box::pin(op(m, e)) as Pin<Box<dyn Future<Output=bool> + Send + 'static>>
        });

        let f = Finding {
            invoke: new_fn,
            regex: r0,
        };
        self.finding.push(f);
    }

    pub async fn run(self) {
        let mut rx = global_receiver();

        while let Ok(e) = rx.recv().await {
            if let Event::GroupMessage(e) = e {
                let msg = e.inner.inner.elements.to_string();

                let r#static: &'static String = unsafe { mem::transmute(&msg) };

                for f in self.finding.iter() {
                    let reg = f.regex.clone();
                    if let Some(m) = reg.find(r#static) {
                        let invoke = &f.invoke;
                        let fu = invoke(m, e.clone());

                        let (s, r) = std::sync::mpsc::channel();
                        tokio::spawn(async move {
                            let con: bool = fu.await;
                            s.send(con).ok();
                        });

                        let con: bool = r.recv().unwrap_or(false);

                        if !con { break; }
                    }
                }
            }
        }
    }
}
*/