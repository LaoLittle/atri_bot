use serde::{Deserialize, Serialize};

pub static DEFAULT_CONFIG: &[u8] = include_bytes!("../../default_config/log.toml");

#[derive(Serialize, Deserialize, Debug)]
pub struct LogConfig {
    pub max_level: Level,
    pub time_format: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            max_level: Level::Info,
            time_format: "[year]-[month]-[day] [hour]:[minute]:[second]".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_tracing_level(&self) -> tracing::Level {
        match self {
            Level::Trace => tracing::Level::TRACE,
            Level::Debug => tracing::Level::DEBUG,
            Level::Info => tracing::Level::INFO,
            Level::Warn => tracing::Level::WARN,
            Level::Error => tracing::Level::ERROR,
        }
    }
}
