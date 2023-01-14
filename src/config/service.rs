use crate::config::service_config_dir_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use tracing::error;

pub struct ServiceConfig<T> {
    path: PathBuf,
    service_name: &'static str,
    default_config: &'static [u8],
    _mark: PhantomData<T>,
}

impl<T> ServiceConfig<T>
where
    for<'a> T: Serialize + Deserialize<'a>,
    T: Default,
{
    pub fn new(name: &'static str, default: &'static [u8]) -> Self {
        Self {
            path: service_config_dir_path().join(format!("{name}.toml")),
            default_config: default,
            service_name: name,
            _mark: PhantomData,
        }
    }

    pub fn read(&self) -> T {
        if self.path.is_file() {
            match fs::read(&self.path) {
                Ok(file) => toml::from_slice(&file).unwrap_or_else(|e| {
                    error!("{e}");
                    let mut path = self.path.clone();
                    path.pop();
                    let mut name = self.service_name.to_owned();
                    name.push_str(".toml.bak");
                    path.push(name);
                    let _ = fs::copy(&self.path, path);
                    self.write_default()
                }),
                Err(e) => {
                    error!("{e}");
                    self.write_default()
                }
            }
        } else {
            self.write_default()
        }
    }

    fn write_default(&self) -> T {
        let default = T::default();
        let _ = fs::write(&self.path, self.default_config);
        default
    }
}
