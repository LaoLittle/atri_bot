extern crate core;

use std::error::Error;

use std::time::Duration;

use atri_bot::service::command::{handle_plugin_command, PLUGIN_COMMAND};
use atri_bot::service::listeners::get_global_worker;
use atri_bot::service::log::init_logger;
use atri_bot::service::login::login_bots;
use atri_bot::service::plugin::PluginManager;
use atri_bot::terminal::{handle_standard_output, start_read_input, PROMPT};
use atri_bot::{get_listener_runtime, Atri};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::{io, signal};
use tracing::{error, info};

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    let _guard = init_logger();
    let mut atri = Atri::new();

    get_listener_runtime().spawn(get_global_worker().start());

    atri.plugin_manager.load_plugins()?;

    let runtime = &atri.global_runtime;

    runtime.spawn(async {
        main0().await.expect("Error");
    });

    runtime.block_on(async {
        tokio::select! {
            result = loop_cli(&mut atri.plugin_manager) => {
                if let Err(e) = result {
                    error!("{}", e);
                }
            }
            result = signal::ctrl_c() => {
                if let Err(e) = result {
                    error!("{}", e);
                }
            }
        }

        Ok::<_, Box<dyn Error>>(())
    })?;

    atri.global_runtime
        .shutdown_timeout(Duration::from_millis(800));

    println!("已成功停止服务");

    Ok(())
}

async fn main0() -> MainResult {
    login_bots().await?;

    Ok(())
}

async fn loop_cli(manager: &mut PluginManager) -> MainResult {
    info!("已启动AtriBot");

    let _out = tokio::task::spawn_blocking(|| {
        if let Err(e) = handle_standard_output() {
            error!("接管Stdout失败: {}", e);
            return false;
        }

        true
    });

    if let Err(e) = start_read_input(manager) {
        error!("初始化命令行服务异常: {}, 命令行可能不会正常工作", e);

        let stdin = io::stdin();
        let mut stdin = BufReader::new(stdin);
        let mut stdout = io::stdout();

        let mut buf = String::new();
        loop {
            buf.clear();
            stdin.read_line(&mut buf).await?;
            let cmd = buf.trim_end();

            match cmd {
                "" => {
                    stdout.write_all(PROMPT).await?;
                    stdout.flush().await?;
                }
                "help" | "?" => {
                    static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                    let mut s = String::from('\n');
                    for &info in INFOS {
                        s.push_str(info);
                        s.push('\n');
                    }
                    s.pop();
                    info!("{}", s);
                }
                "exit" | "quit" | "stop" => {
                    info!("正在停止AtriBot");
                    break;
                }
                plugin if plugin.starts_with(PLUGIN_COMMAND) => {
                    if let Err(e) = handle_plugin_command(plugin, manager) {
                        error!("{}", e);
                    }
                }
                _ => {
                    info!("未知的命令 '{}', 使用 'help' 显示帮助信息", cmd);
                }
            }
        }
    }

    Ok(())
}
