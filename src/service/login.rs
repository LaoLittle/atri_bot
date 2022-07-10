use std::io;
use std::path::Path;

use ricq::{LoginResponse, RQError};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn};

use crate::{Bot, config, get_app};
use crate::bot::BotConfiguration;
use crate::config::login::LoginConfig;

pub async fn login_bots() -> Result<(), RQError> {
    let login_conf_dir = config::login_config_path().await;

    let login_conf = {
        async fn default_config_write<P: AsRef<Path>>(path: P) -> io::Result<LoginConfig> {
            let mut f = fs::File::create(path.as_ref()).await?;
            f.write_all(config::login::DEFAULT_CONFIG).await?;

            Ok(toml::from_slice(config::login::DEFAULT_CONFIG).expect("Shouldn't fail"))
        }

        if login_conf_dir.is_file() {
            let mut f = fs::File::open(&login_conf_dir).await.unwrap();
            let mut s = String::new();
            f.read_to_string(&mut s).await?;

            if let Ok(conf) = toml::from_str(&s) {
                conf
            } else {
                let mut cp = config::service_config_dir_buf();
                cp.push("login.toml.bak");

                fs::copy(&login_conf_dir, cp).await?;
                default_config_write(&login_conf_dir).await?
            }
        } else {
            default_config_write(login_conf_dir).await?
        }
    };

    let mut bots_path = config::bots_dir_buf();
    if !bots_path.is_dir() { fs::create_dir(&bots_path).await?; }

    bots_path.push("0");

    for bot in login_conf.bots {
        if !bot.auto_login { continue; }

        let account = bot.account;
        let pwd = bot.password;

        bots_path.pop();
        bots_path.push(account.to_string());

        bots_path.push("device.json");
        if !bots_path.is_file() {
            warn!("未找到Bot({})的登陆信息，跳过登陆", account);
            continue;
        }
        bots_path.pop();

        let bot = match login_bot(
            account,
            &pwd,
            BotConfiguration {
                work_dir: None,
                version: bot.protocol.unwrap_or(login_conf.default_protocol).as_version(),
            },
        ).await {
            Ok(bot) => bot,
            Err(_) => {
                // todo: optimize
                continue;
            }
        };

        get_app().bots().write().await.push(bot);
    }

    Ok(())
}

async fn login_bot(account: i64, password: &Option<String>, conf: BotConfiguration) -> Result<Bot, RQError> {
    let bot = Bot::new(
        account,
        conf,
    ).await;
    bot.start().await?;

    info!("Bot({})登陆中", account);
    match bot.try_login().await {
        Ok(_) => {
            info!("Bot({})登陆成功", account);
            Ok(bot)
        }
        Err(e) => {
            //error!("Bot({})登陆失败: {:?}", account, e);
            if let Some(pwd) = password {
                info!("Bot({})尝试密码登陆", account);
                let result = bot.client().password_login(account, &pwd).await;
                println!("{:?}", result);
                let mut resp: LoginResponse;
                if let Ok(r) = result {
                    resp = r;
                } else { return Err(e); }

                loop {
                    match resp {
                        LoginResponse::DeviceLockLogin(..) => {
                            let r = if let Ok(r) = bot.client().device_lock_login().await {
                                r
                            } else { return Err(e); };
                            resp = r;
                        }
                        LoginResponse::Success(..) => {
                            info!("Bot({})登陆成功", account);
                            break;
                        }
                        LoginResponse::UnknownStatus(ref s) => {
                            error!("Bot({})登陆失败: {}", account, s.message);
                            return Err(e);
                        }
                        LoginResponse::AccountFrozen => {
                            error!("Bot({})登陆失败: 账号被冻结", account);
                            return Err(e);
                        }
                        _ => {
                            error!("Bot({})登陆失败: {:?}", account, e);
                            return Err(e);
                        }
                    }
                }

                Ok(bot)
            } else {
                Err(e)
            }
        }
    }
}