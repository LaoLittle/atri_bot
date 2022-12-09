pub mod info;
pub mod token;

use dashmap::DashMap;
use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::client::info::BotAccountInfo;
use crate::client::token::Token;
use crate::contact::friend::Friend;
use ricq::client::DefaultConnector;
use ricq::ext::common::after_login;
use ricq::ext::reconnect::{auto_reconnect, Credential};
use ricq::{Client as RQClient, LoginResponse};
use tokio::io;
use tracing::{error, warn};

use crate::contact::group::Group;
use crate::error::{AtriError, AtriResult, LoginError};
use crate::{config, global_status};

#[derive(Clone)]
pub struct Client(Arc<imp::Client>);

impl Client {
    pub async fn new(id: i64, conf: ClientConfiguration) -> Self {
        let b = imp::Client::new(id, conf).await;
        Self(Arc::new(b))
    }

    pub async fn try_login(&self) -> AtriResult<()> {
        let binp = self.work_dir().join("token.bin");

        let token: Token = if let Ok(bytes) = tokio::fs::read(&binp).await {
            prost::Message::decode(&*bytes).expect("Cannot decode token")
        } else if let Ok(f) = tokio::fs::File::open(self.work_dir().join("token.json")).await {
            let std_file = f.into_std().await;
            tokio::task::block_in_place(|| {
                serde_json::from_reader(&std_file).expect("Cannot read token from file")
            })
        } else {
            error!("{}登陆失败: 无法读取Token", self);

            return Err(AtriError::Login(LoginError::TokenNotExist));
        };

        if token.uin != self.id() {
            return Err(AtriError::Login(LoginError::WrongToken));
        }

        let rq_token: ricq::client::Token = token.into();

        let resp = self.0.client.token_login(rq_token).await?;

        if let LoginResponse::Success(..) = resp {
            after_login(&self.0.client).await;

            let info = self.0.client.account_info.read().await;
            self.0.info.get_or_init(|| BotAccountInfo {
                nickname: info.nickname.clone().into(),
                age: info.age.into(),
                gender: info.gender.into(),
            });

            let token = self.gen_token().await;

            tokio::task::spawn_blocking(move || {
                if let Ok(mut file) = std::fs::File::create(&binp) {
                    let proto = prost::Message::encode_to_vec(&token);
                    let _ = file.write_all(&proto);
                }
            });

            self.0.enable.store(true, Ordering::Relaxed);
        } else {
            error!("{}登陆失败: {:?}", self, resp);

            return Err(AtriError::Login(LoginError::TokenLoginFailed));
        }

        Ok(())
    }

    #[inline]
    pub async fn start(&self) -> io::Result<()> {
        let client = self.clone();

        let stream = self.0.connect().await?;

        tokio::spawn(async move {
            client.0.start(stream).await;

            if client.network_status() == 4 {
                let id = client.id();
                global_status().remove_client(id);
                error!("{}因网络原因掉线, 尝试重连", client);

                client
                    .request_client()
                    .stop(ricq::client::NetworkStatus::NetworkOffline);

                let Ok(stream) = client.0.connect().await else {
                    return;
                };

                client.0.start(stream).await;

                if let Err(e) = client.try_login().await {
                    error!("重连登录失败: {}", e);
                    return;
                }

                global_status().add_client(client);
            } else {
                warn!("{}下线", client);
            }
        });

        Ok(())
    }

    pub fn find(id: i64) -> Option<Self> {
        global_status().clients.get(&id).map(|b| b.clone())
    }

    #[inline]
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
        self.request_client().online.load(Ordering::Relaxed)
    }

    pub fn is_heartbeat_enabled(&self) -> bool {
        self.request_client()
            .heartbeat_enabled
            .load(Ordering::Relaxed)
    }

    #[inline]
    pub fn network_status(&self) -> u8 {
        self.0.client.get_status()
    }

    pub async fn reconnect(&self) {
        auto_reconnect(
            self.0.client.clone(),
            Credential::Token(self.request_client().gen_token().await),
            Duration::from_secs(12),
            5,
            DefaultConnector,
        )
        .await;
    }

    pub async fn gen_token(&self) -> Token {
        self.request_client().gen_token().await.into()
    }

    pub fn list() -> Vec<Client> {
        global_status()
            .clients
            .iter()
            .map(|client| client.clone())
            .collect()
    }

    pub async fn refresh_friend_list(&self) -> AtriResult<()> {
        let list = self.request_client().get_friend_list().await?;

        for info in list.friends {
            self.friend_caches()
                .insert(info.uin, Friend::from(self.clone(), info));
        }

        Ok(())
    }

    pub async fn refresh_group_list(&self) -> AtriResult<()> {
        let infos = self.request_client().get_group_list().await?;

        for info in infos {
            let group = Group::from(self.clone(), info);

            self.cache_group(group);
        }

        Ok(())
    }

    pub async fn refresh_group(&self, group_id: i64) -> AtriResult<Option<Group>> {
        let info = self.request_client().get_group_info(group_id).await?;
        if let Some(info) = info {
            let g = Group::from(self.clone(), info);
            self.cache_group(g.clone());
            return Ok(Some(g));
        } else {
            self.group_caches().remove(&group_id);
        }

        Ok(None)
    }

    #[inline]
    pub fn work_dir(&self) -> &Path {
        &self.0.work_dir
    }

    pub fn find_group(&self, id: i64) -> Option<Group> {
        if let Some(g) = self.group_caches().get(&id) {
            return Some(g.clone());
        }

        None
    }

    pub fn find_friend(&self, id: i64) -> Option<Friend> {
        if let Some(f) = self.0.friends.get(&id) {
            return Some(f.clone());
        }

        None
    }

    pub(crate) async fn find_or_refresh_group(&self, id: i64) -> Option<Group> {
        if let Some(g) = self.find_group(id) {
            return Some(g);
        }

        self.refresh_group(id)
            .await
            .map_err(|e| {
                error!("获取群成员({})时发生错误: {}", id, e);

                e
            })
            .unwrap_or(None)
    }

    pub async fn find_or_refresh_friend_list(&self, id: i64) -> Option<Friend> {
        if let Some(f) = self.find_friend(id) {
            return Some(f);
        }

        if let Err(e) = self.refresh_friend_list().await {
            error!("获取好友({})失败: {}", id, e);
        }

        self.find_friend(id)
    }

    pub fn groups(&self) -> Vec<Group> {
        self.group_caches().iter().map(|g| g.clone()).collect()
    }

    pub fn friends(&self) -> Vec<Friend> {
        self.0.friends.iter().map(|f| f.clone()).collect()
    }

    /*pub async fn guild_client(&self) -> GuildClient {
        GuildClient::new(&self.0.client).await
    }*/
}

impl Client {
    #[inline]
    pub(crate) fn friend_caches(&self) -> &DashMap<i64, Friend> {
        &self.0.friends
    }

    #[inline]
    pub(crate) fn group_caches(&self) -> &DashMap<i64, Group> {
        &self.0.groups
    }

    #[inline]
    pub(crate) fn cache_friend(&self, friend: Friend) {
        self.friend_caches().insert(friend.id(), friend);
    }

    #[inline]
    pub(crate) fn cache_group(&self, group: Group) {
        self.group_caches().insert(group.id(), group);
    }

    #[inline]
    pub(crate) fn remove_friend_cache(&self, friend_id: i64) -> Option<Friend> {
        self.friend_caches().remove(&friend_id).map(|(_, f)| f)
    }

    #[inline]
    pub(crate) fn remove_group_cache(&self, group_id: i64) -> Option<Group> {
        self.group_caches().remove(&group_id).map(|(_, g)| g)
    }

    #[inline]
    pub(crate) fn request_client(&self) -> &RQClient {
        &self.0.client
    }
}

impl PartialEq for Client {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id())
            .field("name", &self.nickname())
            .finish()
    }
}

impl Display for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Client({})", self.id())
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
    use ricq::Client as RQClient;
    use tokio::io::AsyncReadExt;
    use tokio::net::{TcpSocket, TcpStream};
    use tokio::task::yield_now;
    use tokio::{fs, io};
    use tracing::{error, warn};

    use crate::channel::GlobalEventBroadcastHandler;
    use crate::client::info::BotAccountInfo;
    use crate::client::ClientConfiguration;
    use crate::contact::friend::Friend;
    use crate::contact::group::Group;

    pub struct Client {
        pub id: i64,
        pub info: OnceLock<BotAccountInfo>,
        pub enable: AtomicBool,
        pub client: Arc<RQClient>,
        pub friends: DashMap<i64, Friend>,
        pub groups: DashMap<i64, Group>,
        pub work_dir: PathBuf,
    }

    impl Client {
        pub async fn new(id: i64, conf: ClientConfiguration) -> Self {
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

            let client = RQClient::new(device, conf.version, GlobalEventBroadcastHandler);
            let client = Arc::new(client);

            Self {
                id,
                info: OnceLock::new(),
                enable: AtomicBool::new(false),
                friends: DashMap::new(),
                groups: DashMap::new(),
                client,
                work_dir,
            }
        }

        pub async fn connect(&self) -> io::Result<TcpStream> {
            let mut servers = self.client.get_address_list().await;

            let total = servers.len();
            let mut times = 0;
            Ok(loop {
                let socket = TcpSocket::new_v4()?;
                let addr = servers
                    .pop()
                    .ok_or_else(|| io::Error::new(ErrorKind::AddrNotAvailable, "重连失败"))?;

                match tokio::time::timeout(Duration::from_secs(2), socket.connect(addr)).await {
                    Ok(Ok(s)) => break s,
                    Ok(Err(e)) => warn!(
                        "连接服务器{}失败, 尝试重连({}/{}): {}",
                        addr, times, total, e
                    ),
                    Err(_) => warn!("连接服务器{}超时, 尝试重连({}/{})", addr, times, total),
                }

                times += 1;
            })
        }

        pub async fn start(&self, stream: TcpStream) {
            let client = self.client.clone();

            let handle = tokio::spawn(async move {
                client.start(stream).await;
            });
            yield_now().await;

            let _ = handle.await;
        }
    }
}

pub struct ClientConfiguration {
    pub work_dir: Option<PathBuf>,
    pub version: ricq::version::Version,
}

impl ClientConfiguration {
    fn work_dir(&self, id: i64) -> PathBuf {
        self.work_dir
            .as_ref()
            .cloned()
            .unwrap_or_else(|| config::clients_dir_path().join(id.to_string()))
    }
}
