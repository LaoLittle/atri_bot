use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::{fs, io, mem, ptr};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::marker::{PhantomData, PhantomPinned};


use std::panic::catch_unwind;
use std::ptr::null_mut;

use libloading::Library;

use tokio::runtime;
use tokio::runtime::Runtime;
use tracing::{error, info, trace, warn};

use crate::error::AtriError;
use atri_ffi::ffi::AtriManager;
use atri_ffi::plugin::{PluginInstance, PluginVTable};

use crate::plugin::ffi::get_plugin_vtable;

pub struct PluginManager {
    plugins: HashMap<usize, Plugin>,
    plugins_path: PathBuf,
    async_runtime: Runtime,
    _mark: PhantomPinned, // move in memory is unsafe because plugin have a pointer to it
    _send: PhantomData<*const ()>, // !send because plugin is unknown
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

        Self {
            plugins: HashMap::new(),
            plugins_path,
            async_runtime,
            _mark: PhantomPinned,
            _send: PhantomData,
        }
    }

    pub fn async_runtime(&self) -> &Runtime {
        &self.async_runtime
    }

    pub fn plugins_path(&self) -> &Path {
        &self.plugins_path
    }

    pub fn find_plugin(&self, handle: usize) -> Option<&Plugin> {
        self.plugins.get(&handle)
    }

    pub fn unload_plugin(&self, name: &str) {
        todo!()
    }

    pub fn load_plugins(&mut self) -> io::Result<()> {
        let plugins_path = self.plugins_path.as_path();
        if !plugins_path.is_dir() {
            fs::create_dir_all(plugins_path)?;
            return Ok(());
        }
        let dir = fs::read_dir(plugins_path)?;

        #[cfg(target_os = "macos")]
        const EXTENSION: &str = "dylib";
        #[cfg(target_os = "windows")]
        const EXTENSION: &str = "dll";
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const EXTENSION: &str = "so";
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

                    if ext_curr
                        .last()
                        .map(|&ext| ext == EXTENSION)
                        .unwrap_or_default()
                    {
                        info!("正在加载插件: {}", name);
                        let result = self.load_plugin(&path);
                        match result {
                            Ok(p) => {
                                match self.plugins.entry(p.handle) {
                                    Entry::Occupied(_old) => {
                                        unsafe {
                                            let lib = ptr::read(&p);
                                            drop(lib);
                                        }
                                        mem::forget(p);
                                        error!("插件({})被重复加载, 这是一个Bug, 请报告此Bug", name);
                                        warn!("未加载插件{}", name);
                                    }
                                    Entry::Vacant(vac) => {
                                        vac.insert(p).enable();
                                    }
                                }

                                info!("插件({})加载成功", name);
                            }
                            Err(e) => {
                                error!("插件({})加载失败: {}", name, e);
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

        let (atri_manager_init, on_init) = unsafe {
            (
                *lib.get::<extern "C" fn(AtriManager)>(b"atri_manager_init")
                    .map_err(|_| AtriError::PluginInitializeError("无法找到插件初始化函数'atri_manager_init', 或许这不是一个插件"))?,
                *lib.get::<extern "C" fn() -> PluginInstance>(b"on_init")
                    .map_err(|_| AtriError::PluginInitializeError("无法找到插件初始化函数'on_init'"))?,
            )
        };
        let handle = atri_manager_init as usize;
        trace!("正在初始化插件");

        atri_manager_init(AtriManager {
            manager_ptr: ptr as *const PluginManager as _,
            handle,
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
                instance: AtomicPtr::new(ptr),
                should_drop,
                vtb: plugin_instance.vtb,
                handle,
                drop_fn,
                _lib: lib,
            };

            plugin
        }).map_err(|_| AtriError::PluginLoadError(String::from("插件加载错误, 可能是插件发生了panic!")))?;

        trace!("正在启用插件");

        Ok(catch)
    }
}

pub struct Plugin {
    enabled: AtomicBool,
    instance: AtomicPtr<()>,
    should_drop: bool,
    vtb: PluginVTable,
    drop_fn: extern "C" fn(*mut ()),
    handle: usize,
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

impl Debug for Plugin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plugin({:p})", self.handle as *const ())
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