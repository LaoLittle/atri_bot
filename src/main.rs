use std::error::Error;
use std::time::Duration;

use atri_bot::service::command::{builtin::handle_plugin_command, PLUGIN_COMMAND};
use atri_bot::service::log::init_logger;
use atri_bot::service::login::login_clients;
use atri_bot::service::plugin::PluginManager;
use atri_bot::{global_listener_runtime, global_listener_worker, terminal, Atri};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::{io, signal};
use tracing::{error, info};

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    // pre-load
    print_welcome_info();
    let _guards = init_logger();
    atri_bot::signal::init_signal_hook();

    // start
    let mut atri = Atri::new();

    global_listener_runtime().spawn(global_listener_worker().start());

    atri.plugin_manager.load_plugins()?;

    let runtime = &atri.runtime;

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

    atri.runtime.shutdown_timeout(Duration::from_millis(800));

    println!("已成功停止服务");

    Ok(())
}

async fn main0() -> MainResult {
    login_clients().await?;

    Ok(())
}

async fn loop_cli(manager: &mut PluginManager) -> MainResult {
    info!("已启动AtriBot");

    let _out = tokio::task::spawn_blocking(|| {
        if let Err(e) = terminal::handle_standard_output() {
            error!("接管Stdout失败: {}", e);
            return false;
        }

        true
    });

    if let Err(e) = terminal::start_read_input(manager) {
        let _ = crossterm::terminal::disable_raw_mode();
        error!("初始化命令行服务异常: {}, 命令行可能不会正常工作", e);

        let stdin = io::stdin();
        let mut stdin = BufReader::new(stdin);
        let mut stdout = io::stdout();

        let mut buf = String::with_capacity(terminal::BUFFER_SIZE);
        loop {
            buf.clear();
            stdin.read_line(&mut buf).await?;
            let cmd = buf.trim_end();

            match cmd {
                "" => {
                    stdout.write_all(terminal::PROMPT).await?;
                    stdout.flush().await?;
                }
                "help" | "?" | "h" => {
                    static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                    let mut s = String::from('\n');
                    for &info in INFOS {
                        s.push_str(info);
                        s.push('\n');
                    }
                    s.pop();
                    info!("{}", s);
                }
                "exit" | "quit" | "stop" | "q" => {
                    terminal::stop_info();
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

fn print_welcome_info() {
    println!(
        "{}",
        include_str!(concat!(env!("OUT_DIR"), "/welcome_info"))
    );
}
