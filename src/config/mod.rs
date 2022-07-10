use std::path::PathBuf;

use tokio::fs;

pub mod login;

static SERVICE_CONFIG_PATH: &str = "service";
static LOGIN_CONFIG_FILE: &str = "login.toml";

pub fn service_config_dir_buf() -> PathBuf {
    PathBuf::from(SERVICE_CONFIG_PATH)
}

pub async fn login_config_path() -> PathBuf {
    let mut buf = PathBuf::new();
    buf.push(SERVICE_CONFIG_PATH);
    if !buf.is_dir() { fs::create_dir(&buf).await.expect("Cannot create dir"); }
    buf.push(LOGIN_CONFIG_FILE);
    buf
}

static BOTS_PATH: &str = "bots";

pub fn bots_dir_buf() -> PathBuf {
    PathBuf::from(BOTS_PATH)
}

static DEFAULT_CONFIG_PATH: &str = "config";
