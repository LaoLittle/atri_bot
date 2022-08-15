use atri_ffi::RustStr;
use tracing::info;

pub extern "C" fn log_info(str: RustStr) {
    info!("{}", str.as_ref());
}
