use super::cast_ref;
use crate::service::plugin::Plugin;
use atri_ffi::RustString;
use tracing::error;

pub extern "C" fn env_get_workspace(handle: usize, _: *const ()) -> RustString {
    let p: &Plugin = cast_ref(handle as *const ());

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

    path.into()
}
