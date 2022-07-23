extern crate core;

use std::error::Error;

use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use atri_qq::{fun, get_runtime, main_handler};
use atri_qq::service::log::{init_logger};
use atri_qq::service::login::login_bots;
use atri_qq::service::plugin::load_plugins;

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    init_logger();

    load_plugins()?;

    let runtime = get_runtime();

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
                static HELP_INFO: &str =
"\
help: Show this info
exit: Exit this program
";
                stdout.write_all(HELP_INFO.as_bytes()).await?;
            }
            "exit" | "quit" | "stop" => {
                println!("Stopping...");
                break;
            },
            _ => {
                println!("Unknown command '{}', use 'help' to show the help info", cmd);
            }
        }
        stdout.write_all(b">>").await?;
        stdout.flush().await?;
    }

    Ok(())
}