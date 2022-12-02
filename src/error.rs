use std::fmt::{Debug, Display, Formatter, Write};
use std::io;

pub type AtriResult<T> = Result<T, AtriError>;

#[derive(Debug)]
pub enum AtriError {
    PluginError(PluginError),
    IO(io::Error),
    Protocol(ricq::RQError),
    Login(LoginError),
}

impl Display for AtriError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login(e) => Display::fmt(e, f),
            Self::IO(e) => {
                f.write_str("io error: ")?;
                Display::fmt(e, f)
            }
            Self::PluginError(e) => Display::fmt(e, f),
            Self::Protocol(e) => Display::fmt(e, f),
        }
    }
}

impl std::error::Error for AtriError {}

#[derive(Debug)]
pub enum LoginError {
    TokenNotExist,
    WrongToken,
    TokenLoginFailed,
}

impl Display for LoginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokenNotExist => f.write_str("token not exist"),
            Self::WrongToken => f.write_str("wrong token"),
            Self::TokenLoginFailed => f.write_str("token login failed. maybe the token is expired"),
        }
    }
}

impl std::error::Error for LoginError {}

#[derive(Debug)]
pub enum PluginError {
    InitializeFail(&'static str),
    LoadFail(String),
    NameConflict,
}

impl Display for PluginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("plugin ")?;
        match self {
            Self::InitializeFail(s) => {
                f.write_str("initialize failed: ")?;
                f.write_str(s)
            }
            Self::LoadFail(s) => {
                f.write_str("load failed, cause: ")?;
                f.write_str(s)
            }
            Self::NameConflict => f.write_str("name conflicted"),
        }
    }
}

impl From<io::Error> for AtriError {
    fn from(err: io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<ricq::RQError> for AtriError {
    fn from(err: ricq::RQError) -> Self {
        Self::Protocol(err)
    }
}

impl From<PluginError> for AtriError {
    fn from(err: PluginError) -> Self {
        Self::PluginError(err)
    }
}
