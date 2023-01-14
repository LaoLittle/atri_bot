use std::path::Path;

pub mod log;
pub mod login;
pub mod plugin;
pub mod service;

pub fn service_config_dir_path() -> &'static Path {
    static SERVICE_CONFIG_PATH: &str = "service";
    Path::new(SERVICE_CONFIG_PATH)
}

pub fn clients_dir_path() -> &'static Path {
    static CLIENTS_PATH: &str = "clients";
    Path::new(CLIENTS_PATH)
}
