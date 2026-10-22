pub mod args;
mod dashboard;
mod management;
pub mod render;
mod run;

use args::{Cli, Command, RunArgs, ServiceCommand};
use atri_bot::control::ControlClient;
use atri_core::ApiError;
use management::Backend;
use std::path::PathBuf;

pub type CliResult<T = ()> = Result<T, CliError>;
#[derive(Debug)]
pub struct CliError {
    pub error: ApiError,
    pub exit_code: u8,
}
impl From<ApiError> for CliError {
    fn from(error: ApiError) -> Self {
        Self {
            error,
            exit_code: 1,
        }
    }
}
impl CliError {
    pub fn timeout(kind: Option<&str>) -> Self {
        Self {
            error: ApiError::new(
                "timeout",
                match kind {
                    Some(kind) => format!("Timed out waiting for event {kind}"),
                    None => "Run lifetime expired".into(),
                },
            ),
            exit_code: 124,
        }
    }
}

pub async fn execute(cli: Cli, bot_dir: PathBuf) -> CliResult {
    let command = cli
        .command
        .unwrap_or_else(|| Command::Run(RunArgs::default()));
    let backend = match command {
        Command::Run(args) => return run::run(bot_dir, args).await,
        Command::Status => {
            print!("{}", management::status(&bot_dir).await?);
            return Ok(());
        }
        Command::Stop
        | Command::Services {
            command: ServiceCommand::Call { .. },
        } => {
            let client = match ControlClient::connect(&bot_dir).await? {
                Some(client) => client,
                None => {
                    let message = match &command {
                        Command::Services {
                            command: ServiceCommand::Call { json, .. },
                        } => {
                            management::service_args(json)?;
                            "Service calls require a running runtime; start it with atri run"
                        }
                        _ => "No active runtime was found",
                    };
                    return Err(ApiError::new("not_running", message).into());
                }
            };
            Backend::Remote(client)
        }
        _ => Backend::connect(&bot_dir).await?,
    };
    let result = management::dispatch(&backend, command).await;
    let closed = backend.close().await;
    match (result, closed) {
        (Err(error), Err(closed)) => {
            eprintln!("shutdown: {}", render::clean(&closed.to_string()));
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Ok(output), Ok(())) => {
            print!("{output}");
            Ok(())
        }
    }
}
