use std::path::Path;

pub mod login;

const SERVICE_CONFIG_PATH: &str = "service";

pub fn service_config_dir_path() -> &'static Path {
    Path::new(SERVICE_CONFIG_PATH)
}

const CLIENTS_PATH: &str = "clients";

pub fn clients_dir_path() -> &'static Path {
    Path::new(CLIENTS_PATH)
}

const _DEFAULT_CONFIG_PATH: &str = "config";
