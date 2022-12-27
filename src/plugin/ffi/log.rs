use super::cast_ref;
use crate::PluginManager;
use atri_ffi::RustStr;

pub extern "C" fn log(handle: usize, manager: *const (), level: u8, str: RustStr) {
    let manager: &PluginManager = cast_ref(manager);
    let str = str.as_ref();
    if let Some(plugin) = manager.find_plugin(handle) {
        match level {
            0 => tracing::trace!("{}: {}", plugin, str),
            1 => tracing::debug!("{}: {}", plugin, str),
            2 => tracing::info!("{}: {}", plugin, str),
            3 => tracing::warn!("{}: {}", plugin, str),
            4 => tracing::error!("{}: {}", plugin, str),
            _ => tracing::info!("{}: {}", plugin, str),
        }
    }
}
