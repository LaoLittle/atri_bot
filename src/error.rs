use std::fmt::{Debug, Display, Formatter};

#[derive(Debug)]
pub enum AtriError {
    PluginInitializeError(&'static str),
    PluginLoadError(String),
}

impl Display for AtriError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for AtriError {}
