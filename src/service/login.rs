use std::io;
use std::path::Path;
use std::time::Duration;

use rand::{thread_rng, Rng};
use ricq::{LoginResponse, RQError};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

use crate::bot::BotConfiguration;
use crate::config::login::{LoginConfig, DEFAULT_CONFIG};
use crate::{config, get_app, Bot};

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

    let bots_path = config::bots_dir_path();
    if !bots_path.is_dir() {
        fs::create_dir(&bots_path).await?;
    }
    let mut logins = vec![];
    for bot in login_conf.bots {
        if !bot.auto_login {
            continue;
        }

        let account = bot.account;
        let pwd = bot.password;

        let bot_path = bots_path.join(account.to_string()).join("device.json");
        if !bot_path.is_file() {
            warn!("未找到Bot({})的登陆信息，跳过登陆", account);
            continue;
        }

        let handle = tokio::spawn(async move {
            match login_bot(
                account,
                &pwd,
                BotConfiguration {
                    work_dir: None,
                    version: bot
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
                    get_app().remove_bot(account);
                    Err(e)
                }
            }
        });
        logins.push(handle);

        let random = { thread_rng().gen_range(0..44) as f32 / 11.2f32 };
        tokio::time::sleep(Duration::from_secs_f32(random)).await;
    }

    for result in logins {
        match result.await.expect("Login panics!") {
            // todo: optimize
            Ok(_) => {}
            Err(_) => {}
        }
    }

    Ok(())
}

async fn login_bot(
    account: i64,
    password: &Option<String>,
    conf: BotConfiguration,
) -> Result<Bot, RQError> {
    let bot = Bot::new(account, conf).await;
    get_app().add_bot(bot.clone());
    bot.start().await?;

    info!("Bot({})登陆中", account);
    match bot.try_login().await {
        Ok(_) => {
            info!("{}登陆成功", bot);
            Ok(bot)
        }
        Err(e) => {
            //error!("Bot({})登陆失败: {:?}", account, e);
            if let Some(pwd) = password {
                info!("{}尝试密码登陆", bot);
                let mut resp = bot.client().password_login(account, pwd).await?;

                loop {
                    match resp {
                        LoginResponse::DeviceLockLogin(..) => {
                            resp = bot.client().device_lock_login().await?;
                        }
                        LoginResponse::Success(..) => {
                            info!("{}登陆成功", bot);
                            let mut dir = bot.work_dir();
                            dir.push("token.json");

                            if let Ok(mut f) = fs::File::create(&dir).await {
                                let token = bot.client().gen_token().await;
                                let s = serde_json::to_string_pretty(&token)
                                    .expect("Cannot serialize token");
                                let _ = f.write_all(s.as_bytes()).await;
                            }

                            break;
                        }
                        LoginResponse::UnknownStatus(ref s) => {
                            error!("{}登陆失败: {}", bot, s.message);
                            return Err(e);
                        }
                        LoginResponse::AccountFrozen => {
                            error!("{}登陆失败: 账号被冻结", bot);
                            return Err(e);
                        }
                        or => {
                            error!("{}登陆失败, 服务器返回: {:?}", bot, or);
                            return Err(e);
                        }
                    }
                }

                Ok(bot)
            } else {
                error!("{}登陆失败: {:?}", bot, e);
                Err(e)
            }
        }
    }
}
