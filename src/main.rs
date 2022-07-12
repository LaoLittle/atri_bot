extern crate core;


use std::error::Error;

use tokio::{io, runtime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use atri_qq::{fun, main_handler};
use atri_qq::service::login::login_bots;

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::filter_fn(|m| {
            match m.level() {
                &Level::TRACE => false,
                _ => true
            }
        }))
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();


    let runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.spawn(main_handler());
    runtime.spawn(fun::handler());

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
        stdout.write_all(b">>").await?;
        stdout.flush().await?;

        let mut buf = String::new();
        stdin.read_line(&mut buf).await?;
        let cmd = buf.trim_end();

        match cmd {
            "" => {
                // nothing to do
            }
            "exit" | "quit" => break,
            _ => {
                println!("Unknown command '{}', use 'help' to show the help info", cmd);
            }
        }
    }

    Ok(())
}