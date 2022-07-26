use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use ricq::{Client, LoginResponse, RQError, RQResult};
use ricq::client::Token;
use ricq::ext::common::after_login;
use ricq::structs::AccountInfo;
use tokio::{fs, io};
use tokio::io::AsyncReadExt;
use tracing::error;

use crate::contact::group::Group;

#[derive(Clone)]
pub struct Bot(Arc<imp::Bot>);

impl Bot {
    pub async fn new(id: i64, conf: BotConfiguration) -> Self {
        let b = imp::Bot::new(id, conf).await;
        Self(Arc::new(b))
    }

    pub async fn try_login(&self) -> RQResult<()> {
        //let client = &self.client;

        let mut p = self.0.work_dir.clone();
        p.push("token.json");

        if p.is_file() {
            let mut f = fs::File::open(&p).await.expect("Cannot open file");

            let mut s = String::new();
            f.read_to_string(&mut s).await?;

            let token: Token = serde_json::from_str(&s).expect("Cannot read token from file");

            let resp = self.0.client.token_login(token).await?;

            if let LoginResponse::Success(..) = resp {
                after_login(&self.0.client).await;
                let client = self.0.client.clone();
                tokio::spawn(async move { client.do_heartbeat().await; });

                self.0.enable.swap(true, Ordering::Relaxed);
            } else {
                error!("Bot({})登陆失败: {:?}", self.0.client.uin().await, resp);

                return Err(RQError::TokenLoginFailed);
            }
        } else {
            return Err(RQError::TokenLoginFailed);
        }

        Ok(())
    }

    pub async fn start(&self) -> io::Result<()> {
        self.0.start().await
    }

    pub fn id(&self) -> i64 {
        self.0.id
    }

    pub async fn nickname(&self) -> String {
        self.0.nickname().await
    }

    pub async fn account_info(&self) -> AccountInfo {
        self.0.account_info().await
    }

    pub async fn refresh_group_list(&self) -> RQResult<()> {
        let infos = self.client().get_group_list().await?;
        self.0.group_list.clear();
        for info in infos {
            self.0.group_list.insert(
                info.code,
                Group::from(self.clone(), info),
            );
        }

        Ok(())
    }

    pub async fn refresh_group_info(&self, group_id: i64) -> RQResult<()> {
        let info = self.client().get_group_info(group_id).await?;
        if let Some(info) = info {
            let g = Group::from(self.clone(), info);
            self.0.group_list.insert(
                group_id,
                g,
            );
        } else {
            self.0.group_list.remove(&group_id);
        }

        Ok(())
    }

    pub fn delete_group(&self, group_id: i64) -> Option<Group> {
        self.0.group_list.remove(&group_id).map(|(_, g)| g)
    }

    pub fn work_dir(&self) -> PathBuf {
        self.0.work_dir.clone()
    }

    pub fn find_group(&self, id: i64) -> Option<Group> {
        self.0.group_list.get(&id).map(|g| g.clone())
    }

    pub(crate) fn client(&self) -> &Client {
        &self.0.client
    }
}

impl PartialEq for Bot {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Debug for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bot")
            .field("id", &self.id())
            .finish()
    }
}

impl Display for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bot({})", self.id())
    }
}

mod imp {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use dashmap::DashMap;
    use ricq::Client;
    use ricq::device::Device;
    use ricq::structs::AccountInfo;
    use tokio::{fs, io};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpSocket;
    use tokio::task::yield_now;
    use tracing::error;

    use crate::bot::BotConfiguration;
    use crate::channel::GlobalEventBroadcastHandler;
    use crate::contact::group::Group;

    pub struct Bot {
        pub id: i64,
        pub enable: AtomicBool,
        pub client: Arc<Client>,
        pub group_list: DashMap<i64, Group>,
        pub work_dir: PathBuf,
    }

    impl Bot {
        pub async fn new(id: i64, conf: BotConfiguration) -> Self {
            let mut work_dir = PathBuf::new();

            if let Some(s) = conf.work_dir {
                work_dir = s;
            } else {
                work_dir.push("bots");
                work_dir.push(id.to_string());
            };

            if !work_dir.is_dir() { fs::create_dir_all(&work_dir).await.expect("Cannot create work dir"); }

            let mut file_p = work_dir.clone();
            file_p.push("device.json");
            let device: Device = if file_p.is_file() {
                if let Ok(fu) = fs::File::open(&file_p)
                    .await
                    .map(|mut f| async move {
                        let mut str = String::new();
                        f.read_to_string(&mut str).await.expect("Cannot read file");

                        match serde_json::from_str(&str) {
                            Ok(d) => d,
                            Err(e) => {
                                error!("{:?}", e);
                                let d = Device::random();

                                let mut bak = file_p.as_os_str().to_owned();
                                bak.push(".bak");
                                let bak = Path::new(&bak);
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

                if let Ok(mut f) = fs::File::create(&file_p).await {
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
                enable: AtomicBool::new(false),
                group_list: DashMap::new(),
                client,
                work_dir,
            }
        }

        pub async fn start(&self) -> io::Result<()> {
            let client = self.client.clone();

            //let addr = SocketAddr::new(Ipv4Addr::new(113, 96, 18, 253).into(), 80);
            let socket = TcpSocket::new_v4()?;
            let stream = socket.connect(client.get_address()).await?;
            //let stream = TcpStream::connect(client.get_address()).await?;

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
    }
}

pub struct BotConfiguration {
    pub work_dir: Option<PathBuf>,
    pub version: &'static ricq::version::Version,
}