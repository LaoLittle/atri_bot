extern crate core;

use std::error::Error;
use std::future::{poll_fn, Future};
use std::mem;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::{io, signal};
use tracing::{error, info};

use atri_qq::event::listener::{Listener, Priority};
use atri_qq::event::GroupMessageEvent;
use atri_qq::service::listeners::get_global_worker;
use atri_qq::service::log::init_logger;
use atri_qq::service::login::login_bots;
use atri_qq::{fun, get_app, get_listener_runtime, Atri};

type MainResult = Result<(), Box<dyn Error>>;

fn main() -> MainResult {
    init_logger();
    let mut atri = Atri::new();

    get_listener_runtime().spawn(get_global_worker().start());

    atri.plugin_manager.load_plugins()?;

    let runtime = &atri.global_runtime;

    let guard = Listener::listening_on_always(|e: GroupMessageEvent| async move {
        if !get_app().check_group_bot(e.group().bot().id(), e.group().id()) {
            e.intercept();
        }
    })
    .set_priority(Priority::Top)
    .start();
    mem::forget(guard);

    //main_handler();
    fun::handler();

    runtime.spawn(async {
        main0().await.expect("Error");
    });

    runtime.block_on(async {
        let mut loop_cli = tokio::spawn(async {
            if let Err(e) = loop_cli().await {
                error!("命令处理服务出现错误: {}", e);
            }
        });
        let mut sig = signal::ctrl_c();

        poll_fn(|ctx| {
            let cli = unsafe { Pin::new_unchecked(&mut loop_cli) };
            let sig = unsafe { Pin::new_unchecked(&mut sig) };

            match (cli.poll(ctx), sig.poll(ctx)) {
                (Poll::Pending, Poll::Pending) => Poll::Pending,
                (Poll::Ready(Err(e)), _) => {
                    error!("{}", e);
                    Poll::Ready(())
                }
                (_, Poll::Ready(result)) => {
                    if let Err(e) = result {
                        error!("{}", e);
                    }
                    println!("正在中止AtriQQ");
                    Poll::Ready(())
                }
                (_, _) => Poll::Ready(()),
            }
        })
        .await;

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

async fn loop_cli() -> MainResult {
    let stdin = io::stdin();
    let mut stdin = BufReader::new(stdin);
    let mut stdout = io::stdout();

    info!("已启动AtriQQ");

    let mut buf = String::new();
    loop {
        buf.clear();
        stdin.read_line(&mut buf).await?;
        let cmd = buf.trim_end();

        match cmd {
            "" => {
                // nothing to do
            }
            "help" | "?" => {
                static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                for &info in INFOS {
                    stdout.write_all(info.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                }
            }
            "exit" | "quit" | "stop" => {
                println!("正在停止AtriQQ");
                break;
            }
            _ => {
                println!("未知的命令 '{}', 使用 'help' 显示帮助信息", cmd);
            }
        }
        stdout.write_all(b">>").await?;
        stdout.flush().await?;
    }

    Ok(())
}
