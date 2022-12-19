pub mod argument;
pub mod builtin;

use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::pin::Pin;

pub enum CommandAction {
    Function(fn(raw: &str) -> CommandResult<()>),
    AsyncFunction(
        fn(raw: &str) -> Pin<Box<dyn Future<Output = CommandResult<()>> + Send + 'static>>,
    ),
    Closure(Box<dyn Fn(&str) -> CommandResult<()> + Send + Sync + 'static>),
    AsyncClosure(CommandHandlerAsync),
    ExternalCFunction(extern "C" fn(raw: atri_ffi::RustStr) -> atri_ffi::error::FFIResult<()>),
    ExternalClosure(atri_ffi::closure::FFIFn<atri_ffi::RustStr, atri_ffi::error::FFIResult<()>>),
    ExternalAsyncClosure(
        atri_ffi::closure::FFIFn<
            atri_ffi::RustStr,
            atri_ffi::future::FFIFuture<atri_ffi::error::FFIResult<()>>,
        >,
    ),
    Complex(HashMap<String, CommandAction>), // cannot change after initialized, or just change the root command.
}

impl CommandAction {
    pub async fn action(&self, args: &str) -> CommandResult<()> {
        match self {
            Self::Function(f) => tokio::task::block_in_place(|| f(args))?,
            Self::AsyncFunction(f) => f(args).await?,
            Self::Closure(f) => tokio::task::block_in_place(|| f(args))?,
            Self::AsyncClosure(f) => f(args).await?,
            Self::ExternalCFunction(f) => Result::from(tokio::task::block_in_place(|| {
                f(atri_ffi::RustStr::from(args))
            }))
            .map_err(CommandError::execute_error)?,
            Self::ExternalClosure(f) => Result::from(tokio::task::block_in_place(|| {
                f.invoke(atri_ffi::RustStr::from(args))
            }))
            .map_err(CommandError::execute_error)?,
            Self::ExternalAsyncClosure(f) => {
                Result::from(f.invoke(atri_ffi::RustStr::from(args)).await)
                    .map_err(CommandError::execute_error)?
            }
            Self::Complex(_commands) => return Err(CommandError::execute_error("")),
        }

        Ok(())
    }
}

type CommandHandlerAsync = Box<
    dyn Fn(&str) -> Pin<Box<dyn Future<Output = CommandResult<()>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug)]
pub enum CommandError {
    MissingArgument(&'static str),
    ExecuteError(Cow<'static, str>),
    IllegalArgument,
}

impl CommandError {
    pub fn execute_error<S: Into<Cow<'static, str>>>(str: S) -> Self {
        Self::ExecuteError(str.into())
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for CommandError {}

impl From<std::num::ParseIntError> for CommandError {
    fn from(_value: std::num::ParseIntError) -> Self {
        Self::IllegalArgument
    }
}

pub const PLUGIN_COMMAND: &str = "plugin";
