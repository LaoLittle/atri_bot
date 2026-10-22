use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "atri",
    version,
    about = "Platform-independent extension runtime",
    long_about = "Run and manage independently deployed native and WebAssembly extensions.\nThe host never builds or deploys source projects automatically."
)]
pub struct Cli {
    /// Deployment directory; defaults to the directory containing this executable.
    #[arg(long, global = true, value_name = "DIRECTORY")]
    pub bot_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start enabled extensions; use the dashboard when attached to a terminal.
    Run(RunArgs),
    /// Manage deployed extensions without starting a platform login.
    #[command(alias = "extensions")]
    Plugins {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Read and update configuration through the runtime (never direct file edits).
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Inspect and cancel framework-managed tasks.
    Tasks {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Discover and invoke generic extension services.
    Services {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Show the running runtime, or the offline deployment state.
    Status,
    /// Gracefully stop a running runtime.
    Stop,
}

#[derive(Debug, Default, Args)]
pub struct RunArgs {
    /// Disable the interactive dashboard even on a terminal.
    #[arg(long)]
    pub headless: bool,
    /// Exit successfully after receiving this exact extension event kind.
    #[arg(long, value_name = "KIND")]
    pub until_event: Option<String>,
    /// Maximum run lifetime in seconds; expiration exits with status 124.
    #[arg(long, value_name = "SECONDS", value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: Option<u64>,
}

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    List,
    Info {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    /// Explicitly install an already-built artifact or extension directory.
    Install {
        path: PathBuf,
    },
    /// Remove deployed extension code; runtime storage is retained.
    Remove {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Show the original configuration document (including comments).
    Show { id: String },
    /// Read a typed value; KEY is a top-level name or JSON Pointer (for example /account/name).
    Get { id: String, key: Option<String> },
    /// Set ID KEY JSON_VALUE; KEY is a top-level name, JSON Pointer, or empty for the document.
    Set {
        id: String,
        key: String,
        value: String,
    },
    /// Edit a private temporary copy using $VISUAL or $EDITOR and commit using CAS.
    Edit { id: String },
    /// Import a document, validating it through the runtime before atomic commit.
    Import {
        id: String,
        path: PathBuf,
        #[arg(long)]
        format: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    List,
    Cancel { id: u64 },
}

#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// List services and methods declared by deployed extensions.
    List,
    /// Invoke a service on the running runtime (no platform-specific host logic).
    Call {
        service: String,
        method: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value = "{}", value_name = "JSON")]
        json: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_run_requires_positive_timeout() {
        assert!(Cli::try_parse_from(["atri", "run", "--timeout", "0"]).is_err());
        let parsed = Cli::try_parse_from([
            "atri",
            "--bot-dir",
            "deployed",
            "run",
            "--headless",
            "--until-event",
            "example.finished",
            "--timeout",
            "15",
        ])
        .unwrap();
        let Some(Command::Run(run)) = parsed.command else {
            panic!("expected run")
        };
        assert!(run.headless);
        assert_eq!(run.until_event.as_deref(), Some("example.finished"));
        assert_eq!(run.timeout, Some(15));
    }
    #[test]
    fn json_pointer_set_and_global_directory_parse() {
        let parsed = Cli::try_parse_from([
            "atri",
            "config",
            "set",
            "example",
            "/nested/key",
            "true",
            "--bot-dir",
            "elsewhere",
        ])
        .unwrap();
        assert_eq!(parsed.bot_dir, Some(PathBuf::from("elsewhere")));
        assert!(matches!(
            parsed.command,
            Some(Command::Config {
                command: ConfigCommand::Set { .. }
            })
        ));
    }
}
