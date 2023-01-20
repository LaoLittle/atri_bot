use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::marker::{PhantomData, PhantomPinned};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::{fs, io};

use libloading::Library;

use tokio::runtime;
use tracing::{error, info, trace, warn};

use crate::error::{AtriError, PluginError};
use atri_ffi::ffi::AtriManager;
use atri_ffi::plugin::{PluginInstance, PluginVTable};

use crate::plugin::plugin_get_function;

const EXTENSION: &str = std::env::consts::DLL_EXTENSION;

pub struct PluginManager {
    pub(crate) plugins: HashMap<String, Box<Plugin>>,
    dependencies: Vec<Library>,
    plugins_path: PathBuf,
    async_runtime: runtime::Runtime,
    _mark: PhantomPinned, // move in memory is unsafe because plugin have a pointer to it
    _send: PhantomData<*const ()>, // !send because plugin may not sendable
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
            dependencies: vec![],
            plugins_path,
            async_runtime,
            _mark: PhantomPinned,
            _send: PhantomData,
        }
    }

    pub fn async_runtime(&self) -> &runtime::Runtime {
        &self.async_runtime
    }

    pub fn plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().map(Box::as_ref).collect()
    }

    pub fn plugins_path(&self) -> &Path {
        &self.plugins_path
    }

    pub fn find_plugin(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name).map(Box::as_ref)
    }

    pub fn load_plugins(&mut self) -> io::Result<()> {
        let mut plugins_path = self.plugins_path.to_path_buf();
        if !plugins_path.is_dir() {
            fs::create_dir_all(&plugins_path)?;
            plugins_path.push("dependencies");
            fs::create_dir(&plugins_path)?;
            return Ok(());
        }
        plugins_path.push("dependencies");
        if !plugins_path.is_dir() {
            fs::create_dir(&plugins_path)?;
        }

        unsafe {
            self.load_dependencies(&plugins_path)?;
        }

        let dep_len = self.dependencies.len();
        if dep_len != 0 {
            info!("已加载{}个依赖", dep_len);
        }

        plugins_path.pop();

        let dir = fs::read_dir(&plugins_path)?;

        for entry in dir {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let f_name = if path.is_file() {
                        path.file_name().unwrap()
                    } else {
                        continue;
                    };

                    let plugin_file_name = f_name.to_str().expect("Unable to get file name");
                    let ext_curr: Vec<&str> = plugin_file_name.split('.').collect();

                    if ext_curr
                        .last()
                        .map(|&ext| ext == EXTENSION)
                        .unwrap_or_default()
                    {
                        info!("正在加载插件: {}", plugin_file_name);
                        let result = self.load_plugin(&path);
                        match result {
                            Ok(p) => {
                                let plugin_display = p.to_string();

                                match self.plugins.entry(p.name().to_owned()) {
                                    Entry::Occupied(_old) => {
                                        error!(
                                            "{} 被重复加载, 这是一个Bug, 请报告此Bug",
                                            plugin_display
                                        );
                                        warn!("未加载 {}", plugin_display);
                                        continue;
                                    }
                                    Entry::Vacant(vac) => {
                                        vac.insert(p).enable();
                                    }
                                }

                                info!("{} 加载成功", plugin_display);
                            }
                            Err(e) => {
                                error!("{} 加载失败: {}", plugin_file_name, e);
                            }
                        };
                    }
                }
                Err(e) => {
                    error!("{:?}", e);
                }
            }
        }

        info!("已加载{}个插件", self.plugins.len());

        Ok(())
    }

    pub fn load_plugin<P: AsRef<OsStr>>(&self, path: P) -> Result<Box<Plugin>, AtriError> {
        let path = Path::new(path.as_ref());
        trace!("正在加载插件动态库, Path={:?}", path);

        let lib_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("unknown file")
            .to_string();

        let lib = unsafe {
            Library::new(path)
                .map_err(|e| PluginError::LoadFail(format!("无法加载插件动态库: {e}")))?
        };

        let (atri_manager_init, on_init) = unsafe {
            (
                *lib.get::<extern "C" fn(AtriManager)>(b"atri_manager_init")
                    .or(Err(PluginError::InitializeFail(
                        "无法找到插件初始化函数'atri_manager_init', 或许这不是一个插件",
                    )))?,
                *lib.get::<extern "C" fn() -> PluginInstance>(b"on_init")
                    .or(Err(PluginError::InitializeFail(
                        "无法找到插件初始化函数'on_init', 或许是插件作者太粗心了?",
                    )))?,
            )
        };

        let mut plugin = Box::<Plugin>::new_uninit();
        let plugin_ptr = plugin.as_ptr();

        let handle = plugin_ptr as usize;
        trace!("正在初始化插件");

        let manager_ptr = self as *const PluginManager;
        atri_manager_init(AtriManager {
            manager_ptr: manager_ptr as *const (),
            handle,
            get_fun: plugin_get_function,
        });

        let plugin_instance = on_init();

        let current = plugin_instance.abi_ver;
        let expected = atri_ffi::plugin::abi_version();

        if expected != current {
            return Err(PluginError::LoadFail(format!(
                "插件ABI版本为{current}, 期望值为{expected}"
            ))
            .into());
        }

        let ptr = (plugin_instance.vtb.new)();

        plugin.write(Plugin {
            enabled: AtomicBool::new(false),
            instance: AtomicPtr::new(ptr),
            vtb: plugin_instance.vtb,
            name: plugin_instance.name.to_string(),
            lib_name,
            should_drop: plugin_instance.should_drop,
            handle,
            manager: manager_ptr,
            _lib: lib,
        });

        Ok(unsafe { plugin.assume_init() })
    }

    unsafe fn load_dependencies<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        let mut p = path.to_path_buf();

        unsafe fn _read(path: &Path, deps: &mut Vec<Library>, p: &mut PathBuf) -> io::Result<()> {
            let dir = fs::read_dir(path)?;
            for entry in dir {
                let entry = entry?;
                p.push(entry.file_name());
                if p.is_file() {
                    if let Some(EXTENSION) = p.extension().map(|os| os.to_str().unwrap_or("")) {
                        match Library::new(&p) {
                            Ok(lib) => {
                                deps.push(lib);
                                trace!("加载依赖({:?})", p);
                            }
                            Err(e) => {
                                warn!("加载依赖动态库失败: {}, 跳过加载", e);
                            }
                        }
                    }
                } else if p.is_dir() {
                    _read(&p.clone(), deps, p)?;
                }
                p.pop();
            }
            Ok(())
        }

        _read(path, &mut self.dependencies, &mut p)?;
        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Plugin {
    enabled: AtomicBool,
    instance: AtomicPtr<()>,
    vtb: PluginVTable,
    name: String,
    lib_name: String,
    should_drop: bool,
    handle: usize,
    manager: *const PluginManager,
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
                new_instance,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    (self.vtb.enable)(new_instance);
                }
                Err(_) => {
                    (self.vtb.drop)(new_instance);
                    return false;
                }
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
            (self.vtb.drop)(ptr);
        } else {
            (self.vtb.disable)(self.instance.load(Ordering::Relaxed));
        }

        true
    }

    pub fn handle(&self) -> usize {
        self.handle
    }

    pub fn should_drop(&self) -> bool {
        self.should_drop
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn manager(&self) -> &PluginManager {
        unsafe { &*self.manager }
    }
}

impl Debug for Plugin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Plugin[{:p}({})]",
            self.handle as *const (), self.lib_name
        )
    }
}

impl Display for Plugin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("插件#")?;
        f.write_str(&self.name)
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        self.disable();
        if !self.should_drop {
            (self.vtb.drop)(self.instance.load(Ordering::Relaxed))
        }
    }
}
