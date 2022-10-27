pub mod builtin;

use crate::PluginManager;
use std::collections::hash_map::Entry;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::mem;
use std::str::FromStr;
use tracing::info;

pub trait Command {
    fn handle(&self) {}

    fn handle_mut(&mut self) {
        self.handle();
    }
}

#[derive(Debug)]
pub enum CommandError {
    UnknownArgument(String),
    MissingField(&'static str),
    ExecuteError(String),
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for CommandError {}

pub type CommandResult<T> = Result<T, CommandError>;

pub const PLUGIN_COMMAND: &str = "plugin";
