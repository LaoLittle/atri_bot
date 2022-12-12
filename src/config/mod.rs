use std::path::{Path, PathBuf};

pub mod login;

const SERVICE_CONFIG_PATH: &str = "service";

pub fn service_config_dir_path() -> &'static Path {
    Path::new(SERVICE_CONFIG_PATH)
}

const CLIENTS_PATH: &str = "clients";

pub fn clients_dir_path() -> &'static Path {
    Path::new(CLIENTS_PATH)
}

pub struct Config {
    file: PathBuf,
}

#[derive(Default)]
pub struct ConfigBuilder<'a> {
    root: PathBuf,
    file: Option<&'a str>,
}

impl<'a> ConfigBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root<P: Into<PathBuf>>(mut self, root: P) -> Self {
        self.root = root.into();
        self
    }

    pub fn with_file(mut self, name: &'a str) -> Self {
        self.file = Some(name);
        self
    }

    pub fn build(self) -> Option<Config> {
        let mut path = self.root;

        path.push(self.file?);

        Some(Config { file: path })
    }
}
