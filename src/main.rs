mod cli;

use clap::Parser;
use std::{io, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    if let Some(code) = atri_bot::crash::maybe_monitor() {
        return code;
    }
    atri_bot::crash::install_panic_hook();
    // Before starting any runtime or loading any extension. Separately linked
    // modules may use their own std panic hook during descriptor discovery.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
    }
    let mut args = match cli::args::Cli::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(code as u8);
        }
    };
    let bot_dir = match resolve_bot_dir(args.bot_dir.take()) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("cannot determine deployment directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let _diagnostics = match atri_bot::crash::install(&bot_dir) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("cannot install crash diagnostics: {error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot create runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(cli::execute(args, bot_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {}", cli::render::clean(&error.error.to_string()));
            ExitCode::from(error.exit_code)
        }
    }
}

fn resolve_bot_dir(override_dir: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(path) = override_dir {
        return Ok(path);
    }
    let executable = std::env::current_exe()?;
    executable
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| io::Error::other("executable path has no parent directory"))
}
