use serde::{Deserialize, Serialize};

pub static DEFAULT_CONFIG: &[u8] = include_bytes!("../../default_config/default_login_conf.toml");

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct LoginConfig {
    pub default_protocol: Protocol,
    #[serde(rename = "bot")]
    pub bots: Vec<BotConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct BotConfig {
    pub account: i64,
    pub password: Option<String>,
    pub protocol: Option<Protocol>,
    #[serde(default = "true_bool")]
    pub auto_login: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum Protocol {
    IPAD,
    AndroidPhone,
    AndroidWatch,
    MacOS,
    QiDian,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::IPAD
    }
}

impl Protocol {
    pub fn as_rq_protocol(&self) -> ricq::version::Protocol {
        use ricq::version::Protocol;
        match self {
            Self::IPAD => Protocol::IPad,
            Self::AndroidPhone => Protocol::AndroidPhone,
            Self::AndroidWatch => Protocol::AndroidWatch,
            Self::MacOS => Protocol::MacOS,
            Self::QiDian => Protocol::QiDian
        }
    }

    pub fn as_version(&self) -> &'static ricq::version::Version {
        ricq::version::get_version(self.as_rq_protocol())
    }
}

fn true_bool() -> bool {
    true
}