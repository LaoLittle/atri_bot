use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ricq::client::Token;
use ricq::ext::common::after_login;
use ricq::structs::AccountInfo;
use ricq::{Client, LoginResponse, RQError, RQResult};

use crate::contact::friend::Friend;
use tokio::{fs, io};
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
        let p = self.0.work_dir.join("token.json");

        if p.is_file() {
            let s = fs::read_to_string(&p).await.expect("Cannot open file");

            let token: Token = serde_json::from_str(&s).expect("Cannot read token from file");

            let resp = self.0.client.token_login(token).await?;

            if let LoginResponse::Success(..) = resp {
                after_login(&self.0.client).await;
                let client = self.0.client.clone();
                tokio::spawn(async move {
                    client.do_heartbeat().await;
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

    pub fn id(&self) -> i64 {
        self.0.id
    }

    pub async fn nickname(&self) -> String {
        self.0.nickname().await
    }

    pub async fn account_info(&self) -> AccountInfo {
        self.0.account_info().await
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

            if let None = self.0.group_list.get(&code) {
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

    pub async fn find_group(&self, id: i64) -> Option<Group> {
        if let Some(g) = self.0.group_list.get(&id) {
            return Some(g.clone());
        }

        let result = self.client().get_group_info(id).await;

        match result {
            Ok(Some(info)) => {
                let group = Group::from(self.clone(), info);
                self.0.group_list.insert(id, group.clone());
                Some(group)
            }
            Ok(None) => None,
            Err(e) => {
                error!("获取群({})信息时发生意料之外的错误: {:?}", id, e);
                None
            }
        }
    }

    pub async fn find_friend(&self, id: i64) -> Option<Friend> {
        if let Some(f) = self.0.friend_list.get(&id) {
            return Some(f.clone());
        }

        None
    }

    pub fn groups(&self) -> Vec<Group> {
        self.0.group_list
            .iter()
            .map(|g| g.clone())
            .collect()
    }

    pub fn friends(&self) -> Vec<Friend> {
        self.0.friend_list
            .iter()
            .map(|f| f.clone())
            .collect()
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
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use dashmap::DashMap;
    use ricq::device::Device;
    use ricq::structs::AccountInfo;
    use ricq::Client;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpSocket;
    use tokio::task::yield_now;
    use tokio::{fs, io};
    use tracing::error;

    use crate::bot::BotConfiguration;
    use crate::channel::GlobalEventBroadcastHandler;
    use crate::contact::friend::Friend;
    use crate::contact::group::Group;

    pub struct Bot {
        pub id: i64,
        pub enable: AtomicBool,
        pub client: Arc<Client>,
        pub friend_list: DashMap<i64, Friend>,
        pub group_list: DashMap<i64, Group>,
        pub work_dir: PathBuf,
    }

    impl Bot {
        pub async fn new(id: i64, conf: BotConfiguration) -> Self {
            let work_dir = conf.get_work_dir(id);

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
                            if let Err(_) = fs::copy(&file_p, &bak).await {
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
                enable: AtomicBool::new(false),
                friend_list: DashMap::new(),
                group_list: DashMap::new(),
                client,
                work_dir,
            }
        }

        pub async fn start(&self) -> io::Result<()> {
            let client = self.client.clone();

            let socket = TcpSocket::new_v4()?;
            let stream = socket.connect(client.get_address_list().await[0]).await?;

            tokio::spawn(async move {
                client.start(stream).await;
            });
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
    pub version: ricq::version::Version,
}

impl BotConfiguration {
    fn get_work_dir(&self, id: i64) -> PathBuf {
        self.work_dir
            .as_ref()
            .cloned()
            .unwrap_or_else(|| PathBuf::new().join("bots").join(id.to_string()))
    }
}
