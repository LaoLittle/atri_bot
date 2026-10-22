use crate::{API_VERSION, ApiError, Value, empty_map, from_value};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub role: ExtensionRole,
    #[serde(default)]
    pub description: String,
    pub api_version: u32,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default)]
    pub services: Vec<ServiceDescriptor>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub configuration: ConfigSpec,
}
impl ExtensionMetadata {
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            role: ExtensionRole::default(),
            description: String::new(),
            api_version: API_VERSION,
            subscriptions: Vec::new(),
            services: Vec::new(),
            capabilities: vec!["config.read".into(), "config.write".into(), "log".into()],
            configuration: ConfigSpec::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRole {
    Adapter,
    #[default]
    Service,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigSpec {
    pub format: String,
    #[serde(default = "empty_map")]
    pub defaults: Value,
    #[serde(default)]
    pub schema: Option<Value>,
}
impl Default for ConfigSpec {
    fn default() -> Self {
        Self {
            format: "toml".into(),
            defaults: empty_map(),
            schema: None,
        }
    }
}
/// Platform-independent event categories; custom names retain their exact wire spelling.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventKind {
    BotOnline,
    BotOffline,
    MessageReceived,
    InteractionRequired,
    Custom(String),
}
impl EventKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::BotOnline => "bot.online",
            Self::BotOffline => "bot.offline",
            Self::MessageReceived => "message.received",
            Self::InteractionRequired => "interaction.required",
            Self::Custom(name) => name,
        }
    }
}
impl From<&str> for EventKind {
    fn from(name: &str) -> Self {
        match name {
            "bot.online" => Self::BotOnline,
            "bot.offline" => Self::BotOffline,
            "message.received" => Self::MessageReceived,
            "interaction.required" => Self::InteractionRequired,
            name => Self::Custom(name.to_owned()),
        }
    }
}
impl From<String> for EventKind {
    fn from(name: String) -> Self {
        match name.as_str() {
            "bot.online" => Self::BotOnline,
            "bot.offline" => Self::BotOffline,
            "message.received" => Self::MessageReceived,
            "interaction.required" => Self::InteractionRequired,
            _ => Self::Custom(name),
        }
    }
}
impl std::fmt::Display for EventKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
impl Serialize for EventKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;
        impl<'de> serde::de::Visitor<'de> for KindVisitor {
            type Value = EventKind;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an event kind string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<EventKind, E> {
                Ok(value.into())
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<EventKind, E> {
                Ok(value.into())
            }
        }
        deserializer.deserialize_str(KindVisitor)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscription {
    pub kind: EventKind,
    #[serde(default)]
    pub priority: i16,
    #[serde(default)]
    pub observer: bool,
}
impl Subscription {
    pub fn observer(kind: EventKind) -> Self {
        Self {
            kind,
            priority: 0,
            observer: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub name: String,
    pub version: u32,
    pub methods: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceBinding {
    pub service: String,
    pub account: Option<String>,
}
/// Authenticated transport principal, assigned by the runtime rather than the caller.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallerKind {
    Runtime,
    Native,
    Wasm,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceCall {
    pub service: String,
    pub method: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default = "empty_map")]
    pub args: Value,
    /// Assigned by the runtime, never trusted from an incoming request.
    #[serde(default)]
    pub caller: String,
    #[serde(default)]
    pub caller_kind: CallerKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishEvent {
    pub kind: EventKind,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default = "empty_map")]
    pub payload: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub source: String,
    pub time_ms: u64,
    pub kind: EventKind,
    pub account: Option<String>,
    pub payload: Value,
}
impl Event {
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        from_value(&self.payload)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventDisposition {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserIdentity {
    pub account: String,
    pub id: String,
    pub display_name: Option<String>,
}
/// Last lifecycle state published by a source for an account.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotState {
    pub source: String,
    pub bot: UserIdentity,
    pub online: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotOnline {
    pub bot: UserIdentity,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotOffline {
    pub account: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Direct,
    Group,
    Channel,
    Thread,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationRef {
    pub account: String,
    pub id: String,
    pub kind: ConversationKind,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageRef {
    pub conversation: ConversationRef,
    pub id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageAuthor {
    pub user: Option<UserIdentity>,
    pub is_self: bool,
    pub is_bot: bool,
    #[serde(default)]
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum MessageContent {
    Text(String),
    Composite(Vec<MessageContent>),
    Mention {
        user: Option<UserIdentity>,
        everyone: bool,
    },
    Reply {
        reference: MessageRef,
        content: Box<MessageContent>,
    },
    Media {
        kind: String,
        handle: String,
        metadata: BTreeMap<String, Value>,
    },
    Forward {
        records: Vec<ForwardRecord>,
    },
    Opaque {
        format: String,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
}

impl From<String> for MessageContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}
impl From<&str> for MessageContent {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}
impl From<Vec<MessageContent>> for MessageContent {
    fn from(parts: Vec<MessageContent>) -> Self {
        Self::Composite(parts)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardRecord {
    pub author: MessageAuthor,
    pub time_ms: u64,
    pub content: MessageContent,
    pub reference: Option<MessageRef>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageReceived {
    pub reference: MessageRef,
    pub author: MessageAuthor,
    pub content: MessageContent,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendMessage {
    pub conversation: ConversationRef,
    pub content: MessageContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionPaths {
    pub root: String,
    pub data: String,
    pub workspace: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigLoad {
    pub defaults: Option<Value>,
    pub format: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub value: Value,
    pub format: String,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigStore {
    pub value: Value,
    pub expected_revision: Option<String>,
    pub format: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigDocument {
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
    pub format: String,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigDocumentStore {
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
    pub format: String,
    pub expected_revision: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageRead {
    pub area: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageWrite {
    pub area: String,
    pub path: String,
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogRecord {
    pub level: LogLevel,
    pub message: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionRequest {
    pub kind: String,
    pub title: String,
    pub content: String,
    pub account: Option<String>,
    pub secret: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SleepRequest {
    pub milliseconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: u64,
    pub name: String,
    pub state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionState {
    Disabled,
    Starting,
    Running,
    Stopping,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub metadata: ExtensionMetadata,
    pub kind: String,
    pub path: String,
    pub state: ExtensionState,
    pub tasks: Vec<TaskRecord>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub extensions: Vec<ExtensionInfo>,
    pub running: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RuntimeNotice {
    Event(Event),
    Log {
        extension: String,
        record: LogRecord,
    },
    Table {
        extension: String,
        table: OutputTable,
    },
    Interaction {
        extension: String,
        request: InteractionRequest,
    },
    State(ExtensionInfo),
}

#[cfg(test)]
mod event_tests {
    use super::*;
    use crate::{decode, encode, to_value};

    #[test]
    fn event_kinds_preserve_known_and_custom_string_wire_values() {
        for (name, kind) in [
            ("bot.online", EventKind::BotOnline),
            ("bot.offline", EventKind::BotOffline),
            ("message.received", EventKind::MessageReceived),
            ("interaction.required", EventKind::InteractionRequired),
            (
                "vendor.new_event",
                EventKind::Custom("vendor.new_event".into()),
            ),
            ("*", EventKind::Custom("*".into())),
            ("", EventKind::Custom(String::new())),
        ] {
            assert_eq!(kind.as_str(), name);
            assert_eq!(kind.to_string(), name);
            assert_eq!(EventKind::from(name), kind);
            assert_eq!(EventKind::from(name.to_owned()), kind);
            assert_eq!(to_value(&kind).unwrap(), Value::Text(name.into()));
            assert_eq!(encode(&kind).unwrap(), encode(name).unwrap());
            assert_eq!(decode::<EventKind>(&encode(name).unwrap()).unwrap(), kind);
        }
        assert!(from_value::<EventKind>(&Value::Integer(1.into())).is_err());
    }

    #[test]
    fn event_envelopes_and_subscriptions_keep_the_original_abi_shape() {
        #[derive(Serialize, Deserialize)]
        struct WireEvent {
            id: u64,
            source: String,
            time_ms: u64,
            kind: String,
            account: Option<String>,
            payload: Value,
        }
        #[derive(Serialize)]
        struct WirePublication<'a> {
            kind: &'a str,
            account: Option<&'a str>,
            payload: Value,
        }
        #[derive(Serialize)]
        struct WireSubscription<'a> {
            kind: &'a str,
            priority: i16,
            observer: bool,
        }
        for name in [
            "bot.online",
            "bot.offline",
            "message.received",
            "interaction.required",
            "custom.unknown",
        ] {
            let wire = WireEvent {
                id: 9,
                source: "provider".into(),
                time_ms: 42,
                kind: name.into(),
                account: Some("account".into()),
                payload: Value::Text("unchanged".into()),
            };
            let bytes = encode(&wire).unwrap();
            let event: Event = decode(&bytes).unwrap();
            assert_eq!(event.kind, EventKind::from(name));
            assert_eq!(encode(&event).unwrap(), bytes);
            let legacy: WireEvent = decode(&encode(&event).unwrap()).unwrap();
            assert_eq!(legacy.kind, name);
            let publication = PublishEvent {
                kind: name.into(),
                account: Some("account".into()),
                payload: Value::Null,
            };
            assert_eq!(
                encode(&publication).unwrap(),
                encode(&WirePublication {
                    kind: name,
                    account: Some("account"),
                    payload: Value::Null
                })
                .unwrap(),
            );
            assert_eq!(
                encode(&Subscription::observer(name.into())).unwrap(),
                encode(&WireSubscription {
                    kind: name,
                    priority: 0,
                    observer: true
                })
                .unwrap(),
            );
        }
    }
}
