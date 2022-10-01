pub mod info;
mod token;

use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ricq::ext::common::after_login;
use ricq::{Client, LoginResponse, RQError, RQResult};

use crate::bot::info::BotAccountInfo;
use crate::bot::token::Token;
use crate::contact::friend::Friend;
use tokio::io;
use tracing::error;

use crate::contact::group::Group;
use crate::get_app;

#[derive(Clone)]
pub struct Bot(Arc<imp::Bot>);

impl Bot {
    pub async fn new(id: i64, conf: BotConfiguration) -> Self {
        let b = imp::Bot::new(id, conf).await;
        Self(Arc::new(b))
    }

    pub async fn try_login(&self) -> RQResult<()> {
        let p = self.0.work_dir.join("token.json");

        if p.is_file() {
            //let s = tokio::fs::read_to_string(&p).await.expect("Cannot open file");
            //let token: ricq::client::Token = serde_json::from_str(&s).expect("Cannot read token from file");

            let token: ricq::client::Token = tokio::task::block_in_place(|| {
                let file = std::fs::File::open(&p).unwrap();
                serde_json::from_reader(&file).expect("Cannot read token from file")
            });

            let resp = self.0.client.token_login(token).await?;

            if let LoginResponse::Success(..) = resp {
                after_login(&self.0.client).await;

                let info = self.0.client.account_info.read().await;
                self.0.info.get_or_init(|| BotAccountInfo {
                    nickname: info.nickname.clone().into(),
                    age: info.age.into(),
                    gender: info.gender.into(),
                });

                let rq = self.client().gen_token().await;
                let token = Token::from(rq);

                tokio::task::spawn_blocking(move || {
                    let mut p = p;

                    if let Ok(file) = std::fs::File::create(&p) {
                        let _ = serde_json::to_writer_pretty(&file, &token);
                    }
                    p.pop();
                    p.push("token.bin");

                    if let Ok(mut file) = std::fs::File::create(&p) {
                        let proto = prost::Message::encode_to_vec(&token);
                        let _ = file.write_all(&proto);
                    }
                });

                self.0.enable.swap(true, Ordering::Relaxed);
            } else {
                error!("{}登陆失败: {:?}", self, resp);

                return Err(RQError::TokenLoginFailed);
            }
        } else {
            error!("{}登陆失败: 未找到Token", self);

            return Err(RQError::TokenLoginFailed);
        }

        Ok(())
    }

    pub async fn start(&self) -> io::Result<()> {
        self.0.start().await
    }

    pub fn find(id: i64) -> Option<Self> {
        get_app().bots.get(&id).map(|b| b.clone())
    }

    pub fn id(&self) -> i64 {
        self.0.id
    }

    pub fn nickname(&self) -> String {
        self.account_info().nickname.read().unwrap().clone()
    }

    pub fn age(&self) -> u8 {
        self.account_info().age.load(Ordering::Relaxed)
    }

    pub fn gender(&self) -> u8 {
        self.account_info().gender.load(Ordering::Relaxed)
    }

    pub fn account_info(&self) -> &BotAccountInfo {
        self.0.info.get().unwrap()
    }

    pub fn is_online(&self) -> bool {
        self.client().online.load(Ordering::Relaxed)
    }

    pub fn list() -> Vec<Bot> {
        get_app().bots.iter().map(|bot| bot.clone()).collect()
    }

    pub async fn refresh_friend_list(&self) -> RQResult<()> {
        let list = self.client().get_friend_list().await?;

        for info in list.friends {
            self.0
                .friend_list
                .insert(info.uin, Friend::from(self.clone(), info));
        }

        Ok(())
    }

    pub async fn refresh_group_list(&self) -> RQResult<()> {
        let infos = self.client().get_group_list().await?;
        self.0.group_list.clear();

        for info in infos {
            let code = info.code;
            let group = Group::from(self.clone(), info);

            if self.0.group_list.get(&code).is_none() {
                self.0.group_list.insert(code, group);
            }
        }

        Ok(())
    }

    pub async fn refresh_group_info(&self, group_id: i64) -> RQResult<()> {
        let info = self.client().get_group_info(group_id).await?;
        if let Some(info) = info {
            let g = Group::from(self.clone(), info);
            self.0.group_list.insert(group_id, g);
        } else {
            self.0.group_list.remove(&group_id);
        }

        Ok(())
    }

    pub(crate) fn delete_friend(&self, friend_id: i64) -> Option<Friend> {
        self.0.friend_list.remove(&friend_id).map(|(_, f)| f)
    }

    pub(crate) fn delete_group(&self, group_id: i64) -> Option<Group> {
        self.0.group_list.remove(&group_id).map(|(_, g)| g)
    }

    pub fn work_dir(&self) -> PathBuf {
        self.0.work_dir.clone()
    }

    pub fn find_group(&self, id: i64) -> Option<Group> {
        if let Some(g) = self.0.group_list.get(&id) {
            return Some(g.clone());
        }

        None
    }

    pub fn find_friend(&self, id: i64) -> Option<Friend> {
        if let Some(f) = self.0.friend_list.get(&id) {
            return Some(f.clone());
        }

        None
    }

    pub fn groups(&self) -> Vec<Group> {
        self.0.group_list.iter().map(|g| g.clone()).collect()
    }

    pub fn friends(&self) -> Vec<Friend> {
        self.0.friend_list.iter().map(|f| f.clone()).collect()
    }

    pub(crate) fn client(&self) -> &Client {
        &self.0.client
    }

    /*pub async fn guild_client(&self) -> GuildClient {
        GuildClient::new(&self.0.client).await
    }*/
}

impl PartialEq for Bot {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Debug for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bot").field("id", &self.id()).finish()
    }
}

impl Display for Bot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bot({})", self.id())
    }
}

mod imp {
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    use dashmap::DashMap;
    use ricq::device::Device;
    use ricq::Client;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpSocket;
    use tokio::task::yield_now;
    use tokio::{fs, io};
    use tracing::error;

    use crate::bot::info::BotAccountInfo;
    use crate::bot::BotConfiguration;
    use crate::channel::GlobalEventBroadcastHandler;
    use crate::contact::friend::Friend;
    use crate::contact::group::Group;

    pub struct Bot {
        pub id: i64,
        pub info: OnceLock<BotAccountInfo>,
        pub enable: AtomicBool,
        pub client: Arc<Client>,
        pub friend_list: DashMap<i64, Friend>,
        pub group_list: DashMap<i64, Group>,
        pub work_dir: PathBuf,
    }

    impl Bot {
        pub async fn new(id: i64, conf: BotConfiguration) -> Self {
            let work_dir = conf.work_dir(id);

            if !work_dir.is_dir() {
                fs::create_dir_all(&work_dir)
                    .await
                    .expect("Cannot create work dir");
            }

            let file_p = work_dir.join("device.json");
            let device: Device = if file_p.is_file() {
                if let Ok(fu) = fs::File::open(&file_p).await.map(|mut f| async move {
                    let mut str = String::new();
                    f.read_to_string(&mut str).await.expect("Cannot read file");

                    match serde_json::from_str(&str) {
                        Ok(d) => d,
                        Err(e) => {
                            error!("{:?}", e);
                            let device = Device::random();

                            let mut bak = file_p.as_os_str().to_owned();
                            bak.push(".bak");
                            let bak = Path::new(&bak);
                            if fs::copy(&file_p, &bak).await.is_err() {
                                return device;
                            };

                            drop(f); // make sure the file is closed

                            tokio::task::block_in_place(|| {
                                if let Ok(f) = std::fs::File::create(file_p) {
                                    serde_json::to_writer_pretty(f, &device).expect(
                                        "Cannot serialize device info Or write device info to file",
                                    );
                                };
                            });

                            device
                        }
                    }
                }) {
                    fu.await
                } else {
                    Device::random()
                }
            } else {
                let device = Device::random();

                tokio::task::block_in_place(|| {
                    if let Ok(f) = std::fs::File::create(&file_p) {
                        serde_json::to_writer_pretty(f, &device)
                            .expect("Cannot serialize device info Or write device info to file");
                    }
                });

                device
            };

            let client = Client::new(device, conf.version, GlobalEventBroadcastHandler);
            let client = Arc::new(client);

            Self {
                id,
                info: OnceLock::new(),
                enable: AtomicBool::new(false),
                friend_list: DashMap::new(),
                group_list: DashMap::new(),
                client,
                work_dir,
            }
        }

        pub async fn start(&self) -> io::Result<()> {
            let client = self.client.clone();

            let mut servers = client.get_address_list().await;

            let total = servers.len();
            let mut times = 0;
            let stream = loop {
                let socket = TcpSocket::new_v4()?;
                let addr = servers
                    .pop()
                    .ok_or(io::Error::new(ErrorKind::AddrNotAvailable, "重连失败"))?;

                if let Ok(Ok(s)) =
                    tokio::time::timeout(Duration::from_secs(2), socket.connect(addr)).await
                {
                    break s;
                } else {
                    times += 1;
                    error!("连接服务器{}失败, 尝试重连({}/{})", addr, times, total);
                }
            };

            tokio::spawn(async move {
                client.start(stream).await;
            });
            yield_now().await;

            Ok(())
        }
    }
}

pub struct BotConfiguration {
    pub work_dir: Option<PathBuf>,
    pub version: ricq::version::Version,
}

impl BotConfiguration {
    fn work_dir(&self, id: i64) -> PathBuf {
        self.work_dir
            .as_ref()
            .cloned()
            .unwrap_or_else(|| PathBuf::new().join("bots").join(id.to_string()))
    }
}
