use super::*;
use crate::{
    ConversationKind, ConversationRef, MessageContent, MessageRef, Operation, SendMessage,
};
use serde::Serialize;

#[derive(Default)]
pub(super) struct ApiState {
    counters: Mutex<BTreeMap<String, i64>>,
    messages: Mutex<BTreeMap<String, Vec<MessageContent>>>,
}
impl ApiState {
    pub(super) fn invoke(&self, call: ServiceCall) -> Result<Value, ApiError> {
        let account = call
            .account
            .as_deref()
            .ok_or_else(|| ApiError::new("missing_account", "select a counter account"))?;
        if account == "denied" {
            return Err(ApiError::new("permission_denied", "account is not granted"));
        }
        if !matches!(account, "first" | "second" | "bad-receipt") {
            return Err(ApiError::new(
                "service_not_found",
                "account has no provider",
            ));
        }
        match (call.service.as_str(), call.method.as_str()) {
            ("probe.counter", "increase") => {
                let operation: Increase = from_value(&call.args)?;
                let mut counters = self.counters.lock().unwrap();
                let counter = counters.entry(account.into()).or_default();
                *counter += operation.by;
                to_value(counter)
            }
            ("atri.messaging", "send") => {
                let message: SendMessage = from_value(&call.args)?;
                if message.conversation.account != account {
                    return Err(ApiError::new("invalid_account", "wrong provider account"));
                }
                let mut messages = self.messages.lock().unwrap();
                let items = messages.entry(account.into()).or_default();
                items.push(message.content);
                let mut conversation = message.conversation;
                if account == "bad-receipt" {
                    conversation.id = "another-peer".into();
                }
                to_value(&MessageRef {
                    conversation,
                    id: format!("{account}-{}", items.len()),
                })
            }
            _ => Err(ApiError::unsupported("unknown operation")),
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
struct Increase {
    by: i64,
}
impl Operation for Increase {
    type Response = i64;
    const SERVICE: &'static str = "probe.counter";
    const METHOD: &'static str = "increase";
}
fn peer(account: &str, id: &str) -> ConversationRef {
    ConversationRef {
        account: account.into(),
        id: id.into(),
        kind: ConversationKind::Group,
    }
}
#[derive(Default)]
struct TypedClient;
#[async_trait]
impl Extension for TypedClient {
    fn metadata() -> ExtensionMetadata {
        let mut metadata = ExtensionMetadata::new("typed-client", "Typed client", "1");
        metadata.capabilities = vec!["services.call".into()];
        metadata
    }
    async fn start(&self, ctx: Context) -> Result<(), ApiError> {
        let first = ctx.account("first");
        let second = ctx.account("second");
        assert_eq!(first.execute(&Increase { by: 3 }).await?, 3);
        assert_eq!(second.execute(&Increase { by: 7 }).await?, 7);
        assert_eq!(first.execute(&Increase { by: -1 }).await?, 2);
        let first_peer = peer("first", "group-a");
        let sent = ctx.conversation(&first_peer).send("first message").await?;
        assert_eq!(sent.conversation, first_peer);
        ctx.conversation(&peer("second", "group-b"))
            .send(String::from("second message"))
            .await?;
        ctx.execute(&SendMessage {
            conversation: peer("first", "group-a"),
            content: "owned request".into(),
        })
        .await?;
        let conflict = SendMessage {
            conversation: peer("second", "group-b"),
            content: "must not be sent".into(),
        };
        assert_eq!(
            first.execute(&conflict).await.unwrap_err().code,
            "invalid_account"
        );
        assert_eq!(
            ctx.account("denied")
                .execute(&Increase { by: 1 })
                .await
                .unwrap_err()
                .code,
            "permission_denied"
        );
        assert_eq!(
            ctx.account("missing")
                .execute(&Increase { by: 1 })
                .await
                .unwrap_err()
                .code,
            "service_not_found"
        );
        assert_eq!(
            ctx.conversation(&peer("bad-receipt", "expected-peer"))
                .send("receipt check")
                .await
                .unwrap_err()
                .code,
            "invalid_response"
        );
        Ok(())
    }
}

#[test]
fn typed_clients_preserve_account_isolation_errors_and_conversation_identity() {
    let mut fixture = Fixture::new::<TypedClient>();
    fixture.start().unwrap();
    let counters = fixture.host.api.counters.lock().unwrap();
    assert_eq!(counters.get("first"), Some(&2));
    assert_eq!(counters.get("second"), Some(&7));
    assert_eq!(counters.len(), 2);
    drop(counters);
    let messages = fixture.host.api.messages.lock().unwrap();
    let first = &messages["first"];
    assert_eq!(first.len(), 2);
    assert!(matches!(&first[0], MessageContent::Text(text) if text == "first message"));
    assert!(matches!(&first[1], MessageContent::Text(text) if text == "owned request"));
    let second = &messages["second"];
    assert_eq!(second.len(), 1);
    assert!(matches!(&second[0], MessageContent::Text(text) if text == "second message"));
    drop(messages);
    fixture.stop();
}
