use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::{fs, io, mem, thread};

use std::panic::catch_unwind;
use std::ptr::null_mut;
use std::sync::Arc;

use libloading::Library;

use tokio::runtime;
use tokio::runtime::Runtime;
use tracing::{error, info, trace};

use crate::error::AtriError;
use atri_ffi::ffi::AtriManager;
use atri_ffi::plugin::{PluginInstance, PluginVTable};

use crate::plugin::ffi::get_plugin_vtable;

pub struct PluginManager {
    plugins: std::sync::Mutex<Vec<Arc<Plugin>>>,
    plugins_path: PathBuf,
    async_runtime: Runtime,
    task: std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>,
    _plugin_handler: thread::JoinHandle<()>,
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
        if !plugins_path.is_dir() {
            let _ = fs::create_dir_all(&plugins_path);
        }

        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();

        let plugin_handler = thread::Builder::new()
            .name("PluginHandler".into())
            .spawn(move || {
                while let Ok(task) = rx.recv() {
                    task();
                }
            })
            .expect("Cannot spawn plugin handler");

        Self {
            plugins: Vec::new().into(),
            plugins_path,
            async_runtime,
            task: tx,
            _plugin_handler: plugin_handler,
        }
    }

    pub fn async_runtime(&self) -> &Runtime {
        &self.async_runtime
    }

    pub fn plugins_path(&self) -> &Path {
        &self.plugins_path
    }

    pub fn enable_plugin(&self, plugin: &Arc<Plugin>) {
        let plugin = plugin.clone();
        let _ = self.task.send(Box::new(move || {
            plugin.enable();
        }));
    }

    pub fn disable_plugin(&self, plugin: &Arc<Plugin>) {
        let plugin = plugin.clone();
        let _ = self.task.send(Box::new(move || {
            plugin.disable();
        }));
    }

    pub fn unload_plugin(&self, name: &str) {
        todo!()
    }

    pub fn load_plugins(&self) -> io::Result<()> {
        let plugins_path = self.plugins_path.as_path();
        if !plugins_path.is_dir() {
            fs::create_dir_all(plugins_path)?;
            return Ok(());
        }
        let dir = fs::read_dir(plugins_path)?;

        #[cfg(target_os = "macos")]
        const EXT: &str = "dylib";
        #[cfg(target_os = "windows")]
        const EXT: &str = "dll";
        #[cfg(all(target_os = "unix", not(target_os = "macos")))]
        const EXT: &str = "so";
        let mut plugins = self.plugins.lock().unwrap();
        for entry in dir {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let f_name = if path.is_file() {
                        path.file_name().unwrap()
                    } else {
                        continue;
                    };

                    let name = f_name.to_str().expect("Unable to get file name");
                    let ext_curr: Vec<&str> = name.split('.').collect();

                    if let Some(&EXT) = ext_curr.last() {
                        info!("正在加载插件: {}", name);
                        let result = self.load_plugin(&path);
                        match result {
                            Ok(p) => {
                                plugins.push(Arc::new(p));
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

    fn load_plugin<P: AsRef<OsStr>>(&self, path: P) -> Result<Plugin, AtriError> {
        trace!("正在加载插件动态库");

        let ptr = self as *const PluginManager as usize;
        let lib = unsafe {
            Library::new(path)
                .map_err(|e| AtriError::PluginLoadError(format!("无法加载插件动态库: {}", e)))?
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.task
                .send(Box::new(move || {
                    let load_plugin =
                        || -> Result<Plugin, AtriError> {
                            let (plugin_init, on_init) = unsafe {
                                (
                                    *lib.get::<extern "C" fn(AtriManager)>(b"atri_manager_init")
                                     .map_err(|_| AtriError::PluginInitializeError("无法找到插件初始化函数'atri_manager_init', 或许这不是一个插件"))?,
                                    *lib.get::<extern "C" fn() -> PluginInstance>(b"on_init")
                                     .map_err(|_| AtriError::PluginInitializeError("无法找到插件初始化函数'on_init'"))?,
                                )
                            };
                            trace!("正在初始化插件");

                            plugin_init(AtriManager {
                                manager_ptr: ptr as *const PluginManager as _,
                                vtb: get_plugin_vtable(),
                            });

                            let catch = catch_unwind(move || {
                                let plugin_instance = on_init();

                                let should_drop = plugin_instance.should_drop;

                                let managed = plugin_instance.instance;
                                let ptr = managed.pointer;
                                let drop_fn = managed.vtable.drop;

                                mem::forget(managed);

                                let plugin = Plugin {
                                    enabled: AtomicBool::new(false),
                                    _lib: lib,
                                    instance: AtomicPtr::new(ptr),
                                    should_drop,
                                    vtb: plugin_instance.vtb,
                                    drop_fn,
                                };

                                plugin.enable();
                                plugin
                            }).map_err(|_| AtriError::PluginLoadError(String::from("插件加载错误, 可能是插件发生了panic!")))?;

                            Ok(catch)
                        };

                    tx.send(load_plugin()).unwrap_or_else(|_| unreachable!());
                }))
                .expect("Cannot send task");

        trace!("正在启用插件");

        let plugin = rx.recv().unwrap_or_else(|_| unreachable!())?;

        Ok(plugin)
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        let (sigtx, sigrx) = std::sync::mpsc::channel();

        let v = mem::take(&mut self.plugins);
        let _ = self.task.send(Box::new(move || {
            drop(v);
            sigtx.send(()).unwrap();
        }));
        sigrx.recv().unwrap();
    }
}

pub struct Plugin {
    enabled: AtomicBool,
    instance: AtomicPtr<()>,
    should_drop: bool,
    vtb: PluginVTable,
    drop_fn: extern "C" fn(*mut ()),
    _lib: Library,
}

impl Plugin {
    pub fn enable(&self) -> bool {
        match self
            .enabled
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => {}
            _ => return false,
        }

        if self.should_drop {
            let initialized = self.instance.load(Ordering::Acquire);
            if !initialized.is_null() {
                (self.vtb.enable)(initialized);
                return true;
            }

            let new_instance = (self.vtb.new)();

            match self.instance.compare_exchange(
                null_mut(),
                new_instance.pointer,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    (self.vtb.enable)(new_instance.pointer);
                    mem::forget(new_instance);
                }
                Err(_) => return false,
            }
        } else {
            (self.vtb.enable)(self.instance.load(Ordering::Relaxed));
        }

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

        if self.should_drop {
            let ptr = self.instance.swap(null_mut(), Ordering::Acquire);
            (self.vtb.disable)(ptr);
            (self.drop_fn)(ptr);
        } else {
            (self.vtb.disable)(self.instance.load(Ordering::Relaxed));
        }

        true
    }

    pub fn should_drop(&self) -> bool {
        self.should_drop
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        self.disable();
        if !self.should_drop {
            (self.drop_fn)(self.instance.load(Ordering::Relaxed))
        }
    }
}
