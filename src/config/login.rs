use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG: &[u8] = include_bytes!("../../default_config/default_login_conf.toml");

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct LoginConfig {
    pub default_protocol: Protocol,
    #[serde(default = "true_bool")]
    pub auto_reconnect: bool,
    #[serde(rename = "client")]
    pub clients: Vec<ClientConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ClientConfig {
    pub account: i64,
    pub password: Option<String>,
    pub protocol: Option<Protocol>,
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
