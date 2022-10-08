use std::error::Error;
use std::fs::File;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{fs, io};

use serde::{Deserialize, Serialize};
use tracing::error;

pub mod command;
pub mod listeners;
pub mod log;
pub mod login;
pub mod plugin_manager;

fn get_service_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let p = PathBuf::from("service");
        let _ = fs::create_dir(&p);
        p
    })
}

pub struct Service {
    name: String,
    path: PathBuf,
}

impl Service {
    pub fn new<S: ToString>(name: S) -> Self {
        let name = name.to_string();
        let mut p = get_service_path().clone();
        let mut s = name.clone();
        p.push(&s);
        s.push_str(".toml");
        p.push(&s);
        Self { name, path: p }
    }

    pub fn with_path<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if !path.is_dir() {
            fs::create_dir_all(path).unwrap();
        }
        self.path = path.join(format!("{}.toml", self.name()));
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn read_config<T>(&self) -> T
    where
        for<'de> T: Serialize + Deserialize<'de> + Default,
    {
        fn _read_config<T>(path: &Path) -> Result<T, Box<dyn Error>>
        where
            for<'de> T: Serialize + Deserialize<'de> + Default,
        {
            let exist = path.is_file();

            let mut f = fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(path)?;

            let data = if exist {
                let mut s = String::new();
                f.read_to_string(&mut s)?;
                toml::from_str(&s)?
            } else {
                let dat = T::default();
                let str = toml::to_string_pretty(&dat)?;
                let _ = f.write_all(str.as_bytes());
                dat
            };

            Ok(data)
        }

        match _read_config(&self.path) {
            Ok(data) => data,
            Err(e) => {
                error!("读取配置文件({:?})时发生意料之外的错误: {}", self.path, e);

                let mut bk = self.path.clone();
                bk.pop();
                bk.push(format!("{}.toml.bak", self.name()));
                let _ = fs::copy(&self.path, bk);

                let data = T::default();
                if let Ok(mut f) = File::create(&self.path) {
                    let s = toml::to_string_pretty(&data).unwrap_or_else(|e| {
                        panic!("Cannot serialize service data: {}, {e}", self.name)
                    });
                    let _ = f.write_all(s.as_bytes());
                }

                data
            }
        }
    }

    pub fn write_config<T>(&self, data: &T)
    where
        T: Serialize,
    {
        fn _write_config<T>(path: &Path, data: &T) -> io::Result<()>
        where
            T: Serialize,
        {
            let mut f = File::create(path)?;
            let s = toml::to_string_pretty(data).expect("Cannot serialize data");

            f.write_all(s.as_bytes())?;
            Ok(())
        }

        if let Err(e) = _write_config(&self.path, data) {
            error!("写入配置文件({:?})时发生意料之外的错误: {}", self.path, e);
        }
    }
}
