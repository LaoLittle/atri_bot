extern crate core;

use ricq::msg::{MessageChain, MessageChainBuilder};
use ricq::version;
use ricq::version::Version;
use std::error::Error;
use std::mem;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::{io, runtime};

use atri_qq::bot::{Bot, BotConfiguration};
use atri_qq::event::listener::{Listener, Priority};
use atri_qq::event::GroupMessageEvent;
use atri_qq::service::listeners::get_global_worker;
use atri_qq::service::log::init_logger;
use atri_qq::service::login::login_bots;
use atri_qq::service::plugin::load_plugins;
use atri_qq::{fun, get_app, get_listener_runtime, get_runtime, main_handler};

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    init_logger();

    get_listener_runtime().spawn(get_global_worker().start());
    load_plugins()?;

    let runtime = get_runtime();

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

    loop {
        let mut buf = String::new();
        stdin.read_line(&mut buf).await?;
        let cmd = buf.trim_end();

        match cmd {
            "" => {
                // nothing to do
            }
            "help" | "?" => {
                static HELP_INFO: &str = "\
help: Show this info
exit: Exit this program
";
                stdout.write_all(HELP_INFO.as_bytes()).await?;
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

#[test]
fn yes() {
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let bot = Bot::new(
            1405685121,
            BotConfiguration {
                work_dir: None,
                version: version::IPAD,
            },
        )
        .await;

        bot.start().await.unwrap();

        bot.try_login().await.unwrap();

        let g = bot.find_group(819281715).await.unwrap();

        let mut chain = MessageChainBuilder::new();
        chain.push_str("你是0我是1");
        for _ in 0..5 {
            let _ = g.send_message(chain.clone().build()).await;
        }
    });
}
