use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG: &[u8] = include_bytes!("../../default_config/login.toml");

/// 登录配置
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct LoginConfig {
    /// 默认登录协议
    pub default_protocol: Protocol,
    /// 是否自动重连
    #[serde(default = "true_bool")]
    pub auto_reconnect: bool,
    /// 所有配置进行登录的客户端
    #[serde(default, rename = "client")]
    pub clients: Vec<ClientConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ClientConfig {
    /// 账号
    pub account: i64,
    /// 密码
    pub password: Option<String>,
    /// 登录协议
    pub protocol: Option<Protocol>,
    /// 是否进行登录
    #[serde(default = "true_bool")]
    pub auto_login: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, Default)]
pub enum Protocol {
    #[default]
    IPAD,
    AndroidPhone,
    AndroidWatch,
    MacOS,
    QiDian,
}

impl Protocol {
    pub fn as_rq_protocol(&self) -> ricq::version::Protocol {
        use ricq::version::Protocol;
        match self {
            Self::IPAD => Protocol::IPad,
            Self::AndroidPhone => Protocol::AndroidPhone,
            Self::AndroidWatch => Protocol::AndroidWatch,
            Self::MacOS => Protocol::MacOS,
            Self::QiDian => Protocol::QiDian,
        }
    }

    pub fn as_version(&self) -> ricq::version::Version {
        ricq::version::get_version(self.as_rq_protocol())
    }
}

const fn true_bool() -> bool {
    true
}
