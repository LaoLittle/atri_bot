use super::cast_ref;
use crate::service::plugin::Plugin;
use atri_ffi::RustStr;

pub extern "C" fn log(handle: usize, _: *const (), level: u8, str: RustStr) {
    let str = str.as_ref();
    let plugin: &Plugin = cast_ref(handle as _);
    match level {
        0 => tracing::trace!("{}: {}", plugin, str),
        1 => tracing::debug!("{}: {}", plugin, str),
        2 => tracing::info!("{}: {}", plugin, str),
        3 => tracing::warn!("{}: {}", plugin, str),
        4 => tracing::error!("{}: {}", plugin, str),
        _ => tracing::info!("{}: {}", plugin, str),
    }
}
