use crate::plugin::cast_ref;
use crate::PluginManager;
use atri_ffi::RustString;
use tracing::info;

pub extern "C" fn log_info(handle: usize, manager: *const (), str: RustString) {
    let manager: &PluginManager = cast_ref(manager);
    let str = String::from(str);
    let plugin = manager.find_plugin(handle).expect("Cannot find plugin");
    info!("{:?}: {}", plugin, str);
}
