//! Versioned, platform-neutral contracts. No Tokio or platform client crosses this API.

#[cfg(not(target_arch = "wasm32"))]
pub mod abi;
mod model;
mod operation;
pub use ciborium::value::Value;
pub use model::*;
pub use operation::Operation;
use serde::{Serialize, de::DeserializeOwned};

pub const API_VERSION: u32 = 1;
pub const MAX_CALL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, serde::Deserialize, thiserror::Error, PartialEq, Eq)]
#[error("{code}: {message}")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self::new("internal", error.to_string())
    }
    pub fn cancelled() -> Self {
        Self::new("cancelled", "extension scope was cancelled")
    }
    pub fn unsupported(operation: impl Into<String>) -> Self {
        Self::new("unsupported", operation)
    }
}

/// Encode without allowing a serializer to allocate an unbounded ABI frame.
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, ApiError> {
    let mut writer = FrameWriter {
        bytes: Vec::new(),
        exceeded: false,
    };
    let result = ciborium::ser::into_writer(value, &mut writer);
    if writer.exceeded {
        return Err(ApiError::new(
            "payload_too_large",
            "ABI payload exceeds 64 MiB",
        ));
    }
    result.map_err(ApiError::internal)?;
    Ok(writer.bytes)
}

struct FrameWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl std::io::Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_CALL_BYTES - self.bytes.len() {
            self.exceeded = true;
            return Err(std::io::Error::other("ABI payload exceeds 64 MiB"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    if bytes.len() > MAX_CALL_BYTES {
        return Err(ApiError::new(
            "payload_too_large",
            "ABI payload exceeds 64 MiB",
        ));
    }
    let mut input = bytes;
    let value = ciborium::de::from_reader(&mut input)
        .map_err(|error| ApiError::new("invalid_payload", error.to_string()))?;
    if !input.is_empty() {
        return Err(ApiError::new(
            "invalid_payload",
            "trailing bytes after CBOR value",
        ));
    }
    Ok(value)
}
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value, ApiError> {
    Value::serialized(value).map_err(ApiError::internal)
}
pub fn from_value<T: DeserializeOwned>(value: &Value) -> Result<T, ApiError> {
    value.deserialized().map_err(ApiError::internal)
}
pub fn empty_map() -> Value {
    Value::Map(Vec::new())
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Request {
    pub method: String,
    #[serde(default = "empty_map")]
    pub params: Value,
}
impl Request {
    pub fn new(method: impl Into<String>, params: impl Serialize) -> Result<Self, ApiError> {
        Ok(Self {
            method: method.into(),
            params: to_value(&params)?,
        })
    }
}

/// Core Wasm ABI operations. Host imports are `atri_v1.call`, `response_len`,
/// `response_read`, `response_drop`; guest exports memory/alloc/free/invoke.
pub mod wasm_abi {
    pub const METADATA: u32 = 0;
    pub const START: u32 = 1;
    pub const EVENT: u32 = 2;
    pub const SERVICE: u32 = 3;
    pub const STOP: u32 = 4;
    pub const IMPORT_MODULE: &str = "atri_v1";
}
