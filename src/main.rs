extern crate core;

use std::error::Error;
use std::mem;

use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use atri_qq::event::listener::{Listener, Priority};
use atri_qq::event::GroupMessageEvent;
use atri_qq::service::listeners::get_global_worker;
use atri_qq::service::log::init_logger;
use atri_qq::service::login::login_bots;
use atri_qq::{fun, get_app, get_listener_runtime, main_handler, Atri};

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    init_logger();
    let atri = Atri::new();

    get_listener_runtime().spawn(get_global_worker().start());

    atri.plugin_manager().load_plugins()?;

    let runtime = atri.global_runtime();

    let guard = Listener::listening_on_always(|e: GroupMessageEvent| async move {
        if !get_app().check_group_bot(e.group().bot().id(), e.group().id()) {
            e.intercept();
        }
    })
    .set_priority(Priority::Top)
    .start();
    mem::forget(guard);

    main_handler();
    fun::handler();

    runtime.spawn(async {
        main0().await.expect("Error");
    });

    runtime.block_on(loop_cli())?;

    Ok(())
}

async fn main0() -> MainResult {
    login_bots().await?;

    Ok(())
}

async fn loop_cli() -> MainResult {
    let stdin = io::stdin();
    let mut stdin = BufReader::new(stdin);
    let mut stdout = io::stdout();

    info!("已启动AtriQQ");

    loop {
        let mut buf = String::new();
        stdin.read_line(&mut buf).await?;
        let cmd = buf.trim_end();

        match cmd {
            "" => {
                // nothing to do
            }
            "help" | "?" => {
                static INFOS: &[&str] = &["help: Show this info", "exit: Exit this program"];

                for &info in INFOS {
                    stdout.write_all(info.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                }
            }
            "exit" | "quit" | "stop" => {
                println!("Stopping...");
                break;
            }
            _ => {
                println!(
                    "Unknown command '{}', use 'help' to show the help info",
                    cmd
                );
            }
        }
        stdout.write_all(b">>").await?;
        stdout.flush().await?;
    }

    Ok(())
}
