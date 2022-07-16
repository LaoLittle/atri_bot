use std::marker::PhantomData;
use std::time::Duration;
use ricq::client::event::GroupMessageEvent;
use ricq::handler::QEvent;
use ricq::msg::MessageChain;
use tokio::time::error::Elapsed;
use crate::global_receiver;

pub struct Filter<T, F>
where F: Fn(&T) -> bool,
F: Send + 'static
{
    inner: F,
_mark: PhantomData<T>
}

impl<T,F> Filter<T, F>
    where F: Fn(&T) -> bool,
          F: Send + 'static
{
    pub fn invoke(&self, val: &T) -> bool {
        true
    }
}

pub async fn next_event<F>(event: &GroupMessageEvent, timeout: Duration, filter: F) -> Result<GroupMessageEvent, Elapsed>
where F: Fn(&GroupMessageEvent) -> bool,
F: Send + 'static
{
    tokio::time::timeout(timeout,async move {
        let mut rx = global_receiver();
        while let Ok(e) = rx.recv().await {
            if let QEvent::GroupMessage(e) = e {
                if event.inner.group_code != e.inner.group_code { continue; }
                if event.inner.from_uin != e.inner.from_uin { continue; }

                if !filter(&e) { continue; }
                return e;
            }
        }

        unreachable!()
    }).await
}

pub async fn next_message<F>(event: &GroupMessageEvent, timeout: Duration, filter: F) -> Result<MessageChain, Elapsed>
where F: Fn(&MessageChain) -> bool,
F: Send + 'static
{
    tokio::time::timeout(timeout,async move {
        let mut rx = global_receiver();
        while let Ok(e) = rx.recv().await {
            if let QEvent::GroupMessage(e) = e {
                if event.inner.group_code != e.inner.group_code { continue; }
                if event.inner.from_uin != e.inner.from_uin { continue; }

                if !filter(&e.inner.elements) { continue; }

                return e.inner.elements;
            }
        }

        unreachable!()
    }).await
}

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