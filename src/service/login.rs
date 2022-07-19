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
    let mut login_conf_dir = config::service_config_dir_buf();
    if !login_conf_dir.is_dir() { fs::create_dir_all(&login_conf_dir).await?; }
    login_conf_dir.push("login.toml");

    let login_conf = {
        async fn default_config_write<P: AsRef<Path>>(path: P) -> io::Result<LoginConfig> {
            let mut f = fs::File::create(path).await?;
            f.write_all(config::login::DEFAULT_CONFIG).await?;

            let default_config = LoginConfig::default();
            Ok(default_config)
        }

        if login_conf_dir.is_file() {
            let mut f = fs::File::open(&login_conf_dir).await?;
            let mut s = String::new();
            f.read_to_string(&mut s).await?;

            match toml::from_str(&s) {
                Ok(conf) => {
                    conf
                }
                Err(e) => {
                    error!("读取登陆配置文件失败: {}", e);

                    let mut cp = config::service_config_dir_buf();
                    cp.push("login.toml.bak");

                    fs::copy(&login_conf_dir, cp).await?;
                    default_config_write(&login_conf_dir).await?
                }
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
            Ok(bot) => {
                if let Err(e) = bot.refresh_group_list().await {
                    warn!("{}刷新群列表失败: {:?}", bot, e);
                }
                bot
            }
            Err(_) => {
                // todo: optimize
                get_app().remove_bot(account);
                continue;
            }
        };
    }

    Ok(())
}

async fn login_bot(account: i64, password: &Option<String>, conf: BotConfiguration) -> Result<Bot, RQError> {
    let bot = Bot::new(
        account,
        conf,
    ).await;
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
                            info!("{}登陆成功", bot);
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
                        _ => {
                            error!("{}登陆失败: {:?}", bot, e);
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