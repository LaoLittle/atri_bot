use super::cast_ref;
use crate::service::plugin::PluginManager;
use atri_ffi::RustString;
use tracing::error;

pub extern "C" fn env_get_workspace(handle: usize, manager: *const ()) -> RustString {
    let manager: &PluginManager = cast_ref(manager);
    manager
        .find_plugin(handle)
        .map(|p| {
            let name = p.name();
            let mut path = std::env::current_dir()
                .ok()
                .and_then(|p| {
                    p.to_str().map(|str| {
                        let mut p = String::from(str);
                        p.push('/');
                        p
                    })
                })
                .unwrap_or_default();

            path.push_str("workspaces/");
            path.push_str(name);

            if let Err(e) = std::fs::create_dir_all(&path) {
                error!("为{}创建Workspace失败: {}", p, e);
            }

            path
        })
        .unwrap_or_else(String::new)
        .into()
}
