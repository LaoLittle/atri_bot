use super::{
    CliError, CliResult,
    args::RunArgs,
    dashboard::{Action, Dashboard},
    render,
};
use atri_bot::Runtime;
use atri_core::{ApiError, RuntimeNotice};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::Duration,
};
use tokio::sync::broadcast;

pub async fn run(bot_dir: PathBuf, args: RunArgs) -> CliResult {
    let interrupt = interrupted();
    tokio::pin!(interrupt);
    let runtime = tokio::select! {
        signal = &mut interrupt => {
            signal?;
            return Ok(());
        }
        runtime = Runtime::open(bot_dir.clone()) => runtime?,
    };
    // Subscribe before starting: startup can publish QR interactions and completion events.
    let mut notices = runtime.subscribe();
    let interactive = !args.headless && io::stdin().is_terminal() && io::stdout().is_terminal();
    let mut dashboard = if interactive {
        Some(Dashboard::open()?)
    } else {
        None
    };
    let result = async {
        tokio::select! {
            signal = &mut interrupt => {
                signal?;
                return Ok(());
            }
            started = runtime.start() => started?,
        }
        if dashboard.is_none() {
            println!("ATRI runtime started. Press Ctrl-C to stop.");
            print!("{}", render::extensions(&runtime.snapshot().await.extensions));
        }
        let timeout = async {
            match args.timeout {
                Some(seconds) => tokio::time::sleep(Duration::from_secs(seconds)).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::pin!(timeout);
        let mut ticks = tokio::time::interval(Duration::from_millis(100));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                received = notices.recv() => {
                    match received {
                        Ok(notice) => {
                            let complete = matches!(&notice, RuntimeNotice::Event(event) if args.until_event.as_deref() == Some(event.kind.as_str()));
                            if let Some(view) = &mut dashboard { view.notice(&notice); } else { render::notice(&notice); }
                            if complete { return Ok(()); }
                        }
                        Err(broadcast::error::RecvError::Lagged(count)) => {
                            if args.until_event.is_some() {
                                return Err(ApiError::new("events_lagged", format!("Lost {count} runtime notices; cannot reliably wait for the requested event")).into());
                            }
                            if let Some(view) = &mut dashboard { view.lagged(count); } else { eprintln!("warning: skipped {count} notices; current state remains available via status"); }
                        }
                        Err(broadcast::error::RecvError::Closed) => return Err(ApiError::new("runtime_closed", "Runtime notice stream closed unexpectedly").into()),
                    }
                }
                signal = &mut interrupt => {
                    signal?;
                    return Ok(());
                }
                _ = &mut timeout => return Err(CliError::timeout(args.until_event.as_deref())),
                _ = ticks.tick() => {
                    if let Some(view) = &mut dashboard {
                        let action = tokio::select! {
                            signal = &mut interrupt => {
                                signal?;
                                return Ok(());
                            }
                            action = view.tick(&runtime, &bot_dir) => action?,
                        };
                        if action == Action::Stop {
                            return Ok(());
                        }
                    }
                    if !runtime.snapshot().await.running {
                        return if let Some(kind) = &args.until_event {
                            Err(ApiError::new("event_not_received", format!("Runtime stopped before event {kind}")).into())
                        } else { Ok(()) };
                    }
                }
            }
        }
    }.await;
    // Restore the terminal even if shutdown itself fails or takes time.
    drop(dashboard);
    let shutdown = runtime.shutdown().await;
    match (result, shutdown) {
        (Err(error), Err(shutdown)) => {
            eprintln!("shutdown: {}", render::clean(&shutdown.to_string()));
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Ok(()), Ok(())) => Ok(()),
    }
}

async fn interrupted() -> Result<(), ApiError> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(ApiError::internal)?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map_err(ApiError::internal),
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.map_err(ApiError::internal)
}
