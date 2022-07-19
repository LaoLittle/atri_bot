use std::{fs, io};
use std::path::PathBuf;

use tracing::error;

static PLUGINS_PATH: &str = "plugins";

pub struct Plugin {}

pub fn plugin_dir_buf() -> PathBuf {
    PathBuf::from(PLUGINS_PATH)
}

pub fn load_plugin() -> io::Result<()> {
    let buf = plugin_dir_buf();
    let dir = fs::read_dir(&buf)?;

    #[allow(unused)]
        #[cfg(target_os = "macos")]
        let ext = "dylib";
    #[cfg(target_os = "windows")]
        let ext = "dll";
    #[cfg(all(target_os = "unix", not(target_os = "macos")))]
        let ext = "so";
    for entry in dir {
        match entry {
            Ok(entry) => {
                let name = entry.file_name().to_str().expect("Unable to get file name");
            }
            Err(e) => {
                error!("{:?}", e);
            }
        }
    }


    Ok(())
}