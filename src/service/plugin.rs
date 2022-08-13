use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, io};

use dashmap::DashMap;
use libloading::Library;
use tokio::runtime;
use tokio::runtime::Runtime;
use tracing::{error, info, trace};

use atri_ffi::ffi::AtriManager;
use atri_ffi::plugin::PluginInstance;

use crate::plugin::ffi::get_plugin_vtable;

pub struct PluginManager {
    plugins: DashMap<String, Plugin>,
    plugins_path: PathBuf,
    async_runtime: Runtime,
}

impl PluginManager {
    pub fn new() -> Self {
        let async_runtime = runtime::Builder::new_multi_thread()
            .worker_threads(12)
            .thread_name("PluginRuntime")
            .enable_all()
            .build()
            .unwrap();

        let plugins_path = PathBuf::from("plugins");

        Self {
            plugins: DashMap::new(),
            plugins_path,
            async_runtime,
        }
    }

    pub fn async_runtime(&self) -> &Runtime {
        &self.async_runtime
    }

    pub fn plugins_path(&self) -> &Path {
        &self.plugins_path
    }

    pub fn load_plugins(&self) -> io::Result<()> {
        let mut buf = self.plugins_path().to_path_buf();
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
                        let result = self.load_plugin(&buf);
                        buf.pop();
                        match result {
                            Ok(p) => {
                                info!("插件({})加载成功", name);
                            }
                            Err(e) => {
                                error!("插件: {} 加载失败: {}", name, e);
                            }
                        };
                    }
                }
                Err(e) => {
                    error!("{:?}", e);
                }
            }
        }

        Ok(())
    }

    fn load_plugin<P: AsRef<OsStr>>(&self, path: P) -> Result<Plugin, libloading::Error> {
        let plugin = unsafe {
            trace!("正在加载插件动态库");
            let lib = Library::new(path)?;
            let plugin_init = lib.get::<extern "C" fn(AtriManager)>(b"atri_manager_init")?;
            let on_init = lib.get::<extern "C" fn() -> PluginInstance>(b"on_init")?;
            trace!("正在初始化插件");
            plugin_init(AtriManager {
                manager_ptr: self as *const PluginManager as _,
                vtb: get_plugin_vtable(),
            });

            let instance = on_init();

            let plugin = Plugin {
                enabled: AtomicBool::new(false),
                _lib: lib,
                instance,
            };

            trace!("正在启用插件");
            plugin.enable();

            plugin
        };

        Ok(plugin)
    }
}

pub struct Plugin {
    enabled: AtomicBool,
    instance: PluginInstance,
    _lib: Library,
}

impl Plugin {
    pub fn enable(&self) -> bool {
        match self
            .enabled
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(false) => {}
            _ => return false,
        }
        self.instance.enable();
        true
    }

    pub fn disable(&self) -> bool {
        match self
            .enabled
            .compare_exchange(true, false, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(true) => {}
            _ => return false,
        }
        self.instance.disable();
        true
    }
}
