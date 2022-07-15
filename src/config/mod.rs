use std::path::PathBuf;

use tokio::fs;

pub mod login;

static SERVICE_CONFIG_PATH: &str = "service";

pub fn service_config_dir_buf() -> PathBuf {
    PathBuf::from(SERVICE_CONFIG_PATH)
}

static BOTS_PATH: &str = "bots";

pub fn bots_dir_buf() -> PathBuf {
    PathBuf::from(BOTS_PATH)
}

static DEFAULT_CONFIG_PATH: &str = "config";