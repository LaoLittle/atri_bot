use crate::service::command::{CommandError, CommandResult, PLUGIN_COMMAND};
use crate::service::plugin::PluginManager;
use std::collections::hash_map::Entry;
use std::mem;
use std::str::FromStr;
use tracing::info;

pub fn handle_plugin_command(
    plugin_command: &str,
    manager: &mut PluginManager,
) -> CommandResult<()> {
    let args: Vec<&str> = plugin_command[PLUGIN_COMMAND.len()..]
        .split(' ')
        .filter(|s| !s.is_empty())
        .collect();

    match *args.first().ok_or(CommandError::MissingArgument(
        "load unload enable disable list",
    ))? {
        "list" => {
            let mut s = String::from('\n');
            for (i, plugin) in manager.plugins().into_iter().enumerate() {
                s.push_str(&format!("{} Plugin(handle={})", i + 1, plugin.handle()));
                s.push('\n');
            }
            info!("已加载的插件: {}", s);
        }
        "load" => {
            let &name = args
                .get(1)
                .ok_or(CommandError::MissingArgument("Plugin name"))?;
            let path = manager.plugins_path().join(name);
            let plugin = manager
                .load_plugin(path)
                .map_err(|e| CommandError::ExecuteError(e.to_string().into()))?;
            match manager.plugins.entry(plugin.handle()) {
                Entry::Vacant(vac) => {
                    vac.insert(plugin).enable();
                }
                _ => return Err(CommandError::ExecuteError("插件不可重复加载".into())),
            }
        }
        "unload" => {
            let &id = args
                .get(1)
                .ok_or(CommandError::MissingArgument("Plugin name"))?;
            let id = usize::from_str(id)?;
            manager
                .plugins
                .remove(&id)
                .ok_or_else(|| CommandError::ExecuteError("未找到插件".into()))?;
            info!("成功卸载插件");
        }
        "reloadAll" => {
            drop(mem::take(&mut manager.plugins));
            manager
                .load_plugins()
                .map_err(|e| CommandError::ExecuteError(e.to_string().into()))?;
        }
        _ => {}
    }

    Ok(())
}
