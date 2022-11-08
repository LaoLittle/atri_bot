use std::fmt::{Debug, Display, Formatter};
use std::io;

pub type AtriResult<T> = Result<T, AtriError>;

#[derive(Debug)]
pub enum AtriError {
    PluginError(PluginError),
    IO(io::Error),
    RQ(ricq::RQError),
    ConnectFailed,
}

#[derive(Debug)]
pub enum PluginError {
    InitializeFail(&'static str),
    LoadFail(String),
    NameConflict,
}

impl Display for AtriError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectFailed => write!(f, "连接失败"),
            or => write!(f, "{:?}", or),
        }
    }
}

impl std::error::Error for AtriError {}

impl From<io::Error> for AtriError {
    fn from(err: io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<ricq::RQError> for AtriError {
    fn from(err: ricq::RQError) -> Self {
        Self::RQ(err)
    }
}

impl From<PluginError> for AtriError {
    fn from(err: PluginError) -> Self {
        Self::PluginError(err)
    }
}
