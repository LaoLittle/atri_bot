use std::path::PathBuf;

pub mod login;

const SERVICE_CONFIG_PATH: &str = "service";

pub fn service_config_dir_buf() -> PathBuf {
    PathBuf::from(SERVICE_CONFIG_PATH)
}

const BOTS_PATH: &str = "bots";

pub fn bots_dir_buf() -> PathBuf {
    PathBuf::from(BOTS_PATH)
}

const DEFAULT_CONFIG_PATH: &str = "config";
