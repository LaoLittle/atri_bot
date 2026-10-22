//! Consumer-facing API handles; the service router remains a transport detail.
use atri_core::{ApiError, ConversationRef, MessageContent, MessageRef, Operation};
use serde::{Serialize, de::DeserializeOwned};

use crate::Context;

impl Context {
    /// Execute a contract-defined operation. Its response type is fixed by the
    /// operation, not chosen by the caller.
    ///
    /// ```compile_fail
    /// # async fn wrong(ctx: &atri_extensions::Context, send: &atri_extensions::SendMessage) -> Result<(), atri_extensions::ApiError> {
    /// let _: String = ctx.execute(send).await?;
    /// # Ok(()) }
    /// ```
    pub async fn execute<O: Operation + ?Sized>(
        &self,
        operation: &O,
    ) -> Result<O::Response, ApiError> {
        self.router()
            .call(O::SERVICE, O::METHOD, operation.account(), operation)
            .await
    }

    pub fn account<'a>(&'a self, account: &'a str) -> Account<'a> {
        Account { ctx: self, account }
    }

    pub fn conversation<'a>(&'a self, reference: &'a ConversationRef) -> Conversation<'a> {
        Conversation {
            ctx: self,
            reference,
        }
    }

    /// Explicit escape hatch for configuration-driven bridges and infrastructure.
    /// Business extensions should use typed operations or conversation handles.
    pub fn router(&self) -> ServiceRouter<'_> {
        ServiceRouter { ctx: self }
    }
}

#[derive(Clone, Copy)]
pub struct Account<'a> {
    ctx: &'a Context,
    account: &'a str,
}
impl Account<'_> {
    pub fn id(&self) -> &str {
        self.account
    }

    pub async fn execute<O: Operation + ?Sized>(
        &self,
        operation: &O,
    ) -> Result<O::Response, ApiError> {
        if operation
            .account()
            .is_some_and(|account| account != self.account)
        {
            return Err(ApiError::new(
                "invalid_account",
                "operation identity conflicts with the selected account",
            ));
        }
        self.ctx
            .router()
            .call(O::SERVICE, O::METHOD, Some(self.account), operation)
            .await
    }
}

#[derive(Clone, Copy)]
pub struct Conversation<'a> {
    ctx: &'a Context,
    reference: &'a ConversationRef,
}
impl Conversation<'_> {
    pub fn reference(&self) -> &ConversationRef {
        self.reference
    }

    /// Send to this conversation, retaining the exact platform/account/peer
    /// identity supplied by the adapter; this is not a quoted reply.
    pub async fn send(&self, content: impl Into<MessageContent>) -> Result<MessageRef, ApiError> {
        // Borrow the identity until serialization rather than cloning its strings.
        #[derive(Serialize)]
        struct Send<'a> {
            conversation: &'a ConversationRef,
            content: MessageContent,
        }
        impl Operation for Send<'_> {
            type Response = MessageRef;
            const SERVICE: &'static str = <atri_core::SendMessage as Operation>::SERVICE;
            const METHOD: &'static str = <atri_core::SendMessage as Operation>::METHOD;
            fn account(&self) -> Option<&str> {
                Some(&self.conversation.account)
            }
        }
        let response = self
            .ctx
            .execute(&Send {
                conversation: self.reference,
                content: content.into(),
            })
            .await?;
        if response.conversation != *self.reference {
            return Err(ApiError::new(
                "invalid_response",
                "provider returned a message reference for another conversation",
            ));
        }
        Ok(response)
    }
}

/// Low-level dynamic routing for bridges whose mapping is supplied at runtime.
#[derive(Clone, Copy)]
pub struct ServiceRouter<'a> {
    ctx: &'a Context,
}
impl ServiceRouter<'_> {
    pub async fn call<A, R>(
        &self,
        service: &str,
        method: &str,
        account: Option<&str>,
        args: &A,
    ) -> Result<R, ApiError>
    where
        A: Serialize + Sync + ?Sized,
        R: DeserializeOwned,
    {
        #[derive(Serialize)]
        struct Call<'a, A: ?Sized> {
            service: &'a str,
            method: &'a str,
            account: Option<&'a str>,
            args: &'a A,
        }
        // Caller identity is assigned by the host. Omit optional untrusted
        // caller fields instead of allocating names that it must overwrite.
        self.ctx
            .invoke(
                "services.call",
                &Call {
                    service,
                    method,
                    account,
                    args,
                },
            )
            .await
    }
}
