use std::path::Path;

pub mod login;

const SERVICE_CONFIG_PATH: &str = "service";

pub fn service_config_dir_path() -> &'static Path {
    Path::new(SERVICE_CONFIG_PATH)
}

const BOTS_PATH: &str = "bots";

pub fn bots_dir_path() -> &'static Path {
    Path::new(BOTS_PATH)
}

const _DEFAULT_CONFIG_PATH: &str = "config";
