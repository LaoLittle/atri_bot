//! Typed operations describe routing once, in API contracts rather than callers.
use serde::{Serialize, de::DeserializeOwned};

use crate::{MessageRef, SendMessage};

/// A serializable operation with a fixed result type and implementation route.
/// Implementations belong in API contract crates, not in service consumers.
pub trait Operation: Serialize + Sync {
    type Response: DeserializeOwned;
    const SERVICE: &'static str;
    const METHOD: &'static str;

    /// An operation may carry its own account identity (for example a message's
    /// conversation). Explicitly account-bound clients must agree with it.
    fn account(&self) -> Option<&str> {
        None
    }
}

impl Operation for SendMessage {
    type Response = MessageRef;
    const SERVICE: &'static str = "atri.messaging";
    const METHOD: &'static str = "send";

    fn account(&self) -> Option<&str> {
        Some(&self.conversation.account)
    }
}
