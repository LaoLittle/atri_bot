use std::{fs, io};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::OnceLock;

use dashmap::DashMap;
use libloading::Library;
use tracing::{error, info, trace};

use crate::plugin::ffi::{get_plugin_vtable, PluginVTable};

static PLUGINS_PATH: &str = "plugins";

#[derive(Default)]
pub struct PluginManager {
    plugins: DashMap<usize, Plugin>,
}

static PLUGIN_MANAGER: OnceLock<PluginManager> = OnceLock::new();

pub fn get_plugin_manager() -> &'static PluginManager {
    PLUGIN_MANAGER.get_or_init(PluginManager::default)
}

pub struct Plugin {
    lib: Library,
}

pub fn plugin_dir_buf() -> PathBuf {
    PathBuf::from(PLUGINS_PATH)
}

pub fn load_plugins() -> io::Result<()> {
    let mut buf = plugin_dir_buf();
    if !buf.is_dir() {
        fs::create_dir_all(buf)?;
        return Ok(());
    }
    let dir = fs::read_dir(&buf)?;

    #[cfg(target_os = "macos")]
    const EXT: &str = "dylib";
    #[cfg(target_os = "windows")]
    const EXT: &str = "dll";
    #[cfg(all(target_os = "unix", not(target_os = "macos")))]
    const EXT: &str = "so";
    for entry in dir {
        match entry {
            Ok(entry) => {
                let f_name = entry.file_name();
                let name = f_name.to_str().expect("Unable to get file name");
                buf.push(name);
                let ext_curr: Vec<&str> = name.split('.').collect();

                if let Some(&EXT) = ext_curr.last() {
                    info!("正在加载插件: {}", name);
                    let result = load_plugin(&buf);
                    buf.pop();
                    match result {
                        Ok(p) => {
                            info!("插件加载成功");
                        }
                        Err(e) => {
                            error!("插件: {} 加载失败: {}", name, e);
                            continue;
                        }
                    };
                } else { continue; }
            }
            Err(e) => {
                error!("{:?}", e);
            }
        }
    }

    Ok(())
}

fn load_plugin<P: AsRef<OsStr>>(path: P) -> Result<Plugin, libloading::Error> {
    let plugin = unsafe {
        trace!("正在加载插件动态库");
        let lib = Library::new(path)?;
        let plugin_init = lib.get::<extern fn(*const PluginVTable)>(b"plugin_init")?;
        trace!("正在初始化插件");
        plugin_init(get_plugin_vtable());

        Plugin {
            lib,
        }
    };

    Ok(plugin)
}