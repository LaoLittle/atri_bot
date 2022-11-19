use std::io;
use std::path::Path;
use std::time::Duration;

use rand::{thread_rng, Rng};
use ricq::{LoginResponse, RQError};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

use crate::client::BotConfiguration;
use crate::config::login::{LoginConfig, DEFAULT_CONFIG};
use crate::error::AtriResult;
use crate::{config, global_status, Client};

pub async fn login_bots() -> Result<(), RQError> {
    let login_conf_dir = {
        let path = config::service_config_dir_path();
        if !path.is_dir() {
            fs::create_dir_all(&path).await?;
        }
        path
    }
    .join("login.toml");

    let login_conf = {
        async fn default_config_write<P: AsRef<Path>>(path: P) -> io::Result<LoginConfig> {
            fs::write(path, DEFAULT_CONFIG).await?;

            let default_config = LoginConfig::default();
            Ok(default_config)
        }

        if login_conf_dir.is_file() {
            let s = fs::read_to_string(&login_conf_dir).await?;

            match toml::from_str(&s) {
                Ok(conf) => conf,
                Err(e) => {
                    error!("读取登陆配置文件失败: {}", e);

                    let cp = config::service_config_dir_path().join("login.toml.bak");

                    fs::copy(&login_conf_dir, cp).await?;
                    default_config_write(&login_conf_dir).await?
                }
            }
        } else {
            default_config_write(login_conf_dir).await?
        }
    };

    let clients_path = config::clients_dir_path();
    if !clients_path.is_dir() {
        fs::create_dir(&clients_path).await?;
    }
    let mut logins = vec![];
    for client in login_conf.clients {
        if !client.auto_login {
            continue;
        }

        let account = client.account;
        let pwd = client.password;

        let bot_path = clients_path.join(account.to_string()).join("device.json");
        if !bot_path.is_file() {
            warn!("未找到Client({})的登陆信息，跳过登陆", account);
            continue;
        }

        let handle = tokio::spawn(async move {
            match login_bot(
                account,
                &pwd,
                BotConfiguration {
                    work_dir: None,
                    version: client
                        .protocol
                        .unwrap_or(login_conf.default_protocol)
                        .as_version(),
                },
            )
            .await
            {
                Ok(bot) => {
                    if let Err(e) = bot.refresh_friend_list().await {
                        warn!("{}刷新好友列表失败: {:?}", bot, e);
                    }
                    if let Err(e) = bot.refresh_group_list().await {
                        warn!("{}刷新群列表失败: {:?}", bot, e);
                    }
                    Ok(bot)
                }
                Err(e) => {
                    global_status().remove_client(account);
                    Err(e)
                }
            }
        });
        logins.push(handle);

        let random = { thread_rng().gen_range(0..44) as f32 / 11.2f32 };
        tokio::time::sleep(Duration::from_secs_f32(random)).await;
    }

    for result in logins {
        match result.await {
            Ok(Ok(client)) => {
                global_status().add_client(client.clone());
                info!("{}登陆成功", client);
            }
            Ok(Err(_)) => {
                //error!("登录失败", e);
            }
            Err(e) => {
                error!("登录时发生意料之外的错误: {}", e);
            }
        }
    }

    Ok(())
}

async fn login_bot(
    account: i64,
    password: &Option<String>,
    conf: BotConfiguration,
) -> AtriResult<Client> {
    let client = Client::new(account, conf).await;
    client.start().await?;

    info!("Client({})登陆中", account);
    match client.try_login().await {
        Ok(_) => Ok(client),
        Err(e) => {
            error!("Bot({})登陆失败: {}", account, e);
            if let Some(pwd) = password {
                info!("{}尝试密码登陆", client);
                let mut resp = client.request_client().password_login(account, pwd).await?;

                loop {
                    match resp {
                        LoginResponse::DeviceLockLogin(..) => {
                            resp = client.request_client().device_lock_login().await?;
                        }
                        LoginResponse::Success(..) => {
                            let tokenp = client.work_dir().join("token.json");

                            if let Ok(mut f) = fs::File::create(&tokenp).await {
                                let token = client.request_client().gen_token().await;
                                let s = serde_json::to_string_pretty(&token)
                                    .expect("Cannot serialize token");
                                let _ = f.write_all(s.as_bytes()).await;
                            }

                            break;
                        }
                        LoginResponse::UnknownStatus(ref s) => {
                            error!("{}登陆失败: {}", client, s.message);
                            return Err(e);
                        }
                        LoginResponse::AccountFrozen => {
                            error!("{}登陆失败: 账号被冻结", client);
                            return Err(e);
                        }
                        or => {
                            error!("{}登陆失败, 服务器返回: {:?}", client, or);
                            return Err(e);
                        }
                    }
                }

                Ok(client)
            } else {
                error!("{}登陆失败: {:?}", client, e);
                Err(e)
            }
        }
    }
}
