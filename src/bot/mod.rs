use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

use ricq::{Client, LoginResponse, RQError, RQResult};
use ricq::client::Token;
use ricq::device::Device;
use ricq::ext::common::after_login;
use ricq::structs::AccountInfo;
use tokio::{fs, io};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::yield_now;
use tracing::error;

use crate::channel::GlobalEventBroadcastHandler;

#[derive(Clone)]
pub struct Bot {
    id: i64,
    client: Arc<Client>,
    work_dir: Arc<String>,
}


impl Bot {
    pub async fn new(id: i64, conf: BotConfiguration) -> Self {
        let mut buf = PathBuf::new();

        let work_dir = if let Some(s) = conf.work_dir {
            buf.push(&s);
            s
        } else {
            let s = format!("bots/{}", id);
            buf.push(&s);
            s
        };

        if !buf.is_dir() { fs::create_dir_all(&buf).await.expect("Cannot create work dir"); }

        buf.push("device.json");
        let device: Device = if buf.is_file() {
            let file_p = buf.clone();

            if let Ok(fu) = fs::File::open(&buf)
                .await
                .map(|mut f| async move {
                    let mut str = String::new();
                    f.read_to_string(&mut str).await.expect("Cannot read file");

                    match serde_json::from_str(&str) {
                        Ok(d) => d,
                        Err(e) => {
                            println!("{:?}", e);
                            let d = Device::random();

                            let mut bak = file_p.clone();
                            bak.pop();
                            bak.push("device.json.bak");
                            if let Err(_) = fs::copy(&file_p, &bak).await { return d; };

                            drop(f); // make sure the file is closed
                            if let Ok(mut f) = fs::File::create(file_p).await {
                                let s = serde_json::to_string_pretty(&d).expect("Cannot serialize device info");

                                f.write_all(s.as_bytes()).await.expect("Cannot write device info to file");
                            };

                            d
                        }
                    }
                }) { fu.await } else { Device::random() }
        } else {
            let d = Device::random();

            if let Ok(mut f) = fs::File::create(&buf).await {
                let s = serde_json::to_string_pretty(&d).unwrap();

                f.write_all(s.as_bytes()).await.expect("Cannot write device info to file")
            }

            d
        };

        let client = Client::new(
            device,
            conf.version,
            GlobalEventBroadcastHandler,
        );
        let client = Arc::new(client);

        Self {
            id,
            client,
            work_dir: Arc::new(work_dir),
        }
    }

    pub async fn try_login(&self) -> RQResult<()> {
        //let client = &self.client;

        let mut p = PathBuf::new();
        p.push(&*self.work_dir);
        p.push("token.json");

        if p.is_file() {
            let mut f = fs::File::open(&p).await.expect("Cannot open file");

            let mut s = String::new();
            f.read_to_string(&mut s).await?;

            let token: Token = serde_json::from_str(&s).expect("Cannot read token from file");

            let resp = self.client.token_login(token).await?;

            if let LoginResponse::Success(..) = resp {
                after_login(&self.client).await;
                let client = self.client.clone();
                tokio::spawn(async move { client.do_heartbeat().await; });

                return Ok(());
            }

            error!("Bot({})登陆失败: {:?}", self.client.uin().await, resp);

            return Err(RQError::TokenLoginFailed);
        };

        Err(RQError::TokenLoginFailed)
    }

    pub async fn start(&self) -> io::Result<()> {
        let client = self.client.clone();

        //let addr = SocketAddr::new(Ipv4Addr::new(113, 96, 18, 253).into(), 80);
        let stream = TcpStream::connect(client.get_address()).await?;

        tokio::spawn(async move { client.start(stream).await; });
        yield_now().await;

        Ok(())
    }

    pub async fn nickname(&self) -> String {
        self.client.account_info.read().await.nickname.clone()
    }

    pub async fn account_info(&self) -> AccountInfo {
        let info = self.client.account_info.read().await;

        AccountInfo {
            nickname: info.nickname.clone(),
            age: info.age,
            gender: info.gender,
        }
    }

    pub async fn logout(&self) {}

    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl Bot {
    pub fn id(&self) -> i64 {
        self.id
    }
}

impl Debug for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bot")
            .field("id", &self.id)
            .finish()
    }
}

pub struct BotConfiguration {
    pub work_dir: Option<String>,
    pub version: &'static ricq::version::Version,
}