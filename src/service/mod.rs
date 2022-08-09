
use std::error::Error;
use std::fs::File;


use std::{fs, io, mem};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub mod login;
pub mod plugin;
pub mod log;
pub mod command;
pub mod listeners;

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
    handler: Box<dyn ServiceHandler>,
}

impl Service {
    pub fn new<S: ToString>(name: S) -> Self {
        let name = name.to_string();
        let mut p = get_service_path().clone();
        p.push(&name);
        Self {
            name,
            path: p,
            handler: Box::new(())
        }
    }

    pub fn with_path(&mut self,mut path: PathBuf) {
        if !path.is_dir() { fs::create_dir(&path).unwrap(); }
        path.push(format!("{}.toml", self.name()));
        self.path = path;
    }

    pub fn with_handler<H: ServiceHandler>(&mut self, handler: H) -> &mut Self {
        let handler: Box<dyn ServiceHandler> = Box::new(handler);

        self.handler = handler;
        self
    }
    
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn read_config<T>(&self) -> T
    where for<'a> T: Serialize + Deserialize<'a> + Default
    {
        fn _read_config<T>(path: &Path) -> Result<T, Box<dyn Error>>
            where for<'a> T: Deserialize<'a> + Default
        {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(path)?;

            let mut s = String::new();
            f.read_to_string(&mut s)?;

            let data = toml::from_str(&s)?;
            Ok(data)
        }

        match _read_config(&self.path) {
            Ok(data) => data,
            Err(e) => {
                error!("读取配置文件({:?})时发生意料之外的错误: {}", self.path, e);

                let data = T::default();
                if let Ok(mut f) = File::create(&self.path) {
                    let s = toml::to_string_pretty(&data).expect(&format!("Cannot serialize service data: {}", self.name));
                    let _ = f.write_all(s.as_bytes());
                }

                data
            }
        }
    }

    pub fn write_config<T>(&self, data: &T)
    where T: Serialize
    {
        fn _write_config<T>(path: &Path, data: &T) -> io::Result<()>
        where T: Serialize
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

    pub fn start(self) -> Arc<Self> {
        info!("正在启动{}服务", self.name);
        let arc = Arc::new(self);
        arc.handler.on_start(&arc);
        arc
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let mut handler: Box<dyn ServiceHandler> = Box::new(());
        mem::swap(&mut handler, &mut self.handler);
        handler.on_close(self);
    }
}

pub trait ServiceHandler: 'static {
    fn on_start(&self, service: &Arc<Service>);

    fn on_close(&mut self, service: &Service) {
        //nop
    }
}

impl ServiceHandler for () {
    fn on_start(&self, _: &Arc<Service>) {
        //nop
    }
}