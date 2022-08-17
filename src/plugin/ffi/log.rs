use atri_ffi::{RustString};
use tracing::info;

pub extern "C" fn log_info(str: RustString) {
    let str = String::from(str);
    info!("Plugin: {}", str);
}
