use serde::{Deserialize, Serialize};

/// Diagnostic context only. Configuration, message payloads and credentials are excluded.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrashSnapshot {
    pub version: u32,
    pub pid: u32,
    pub executable: String,
    pub threads: Vec<ThreadSnapshot>,
    pub tasks: Vec<TaskSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadSnapshot {
    pub os_thread_id: u64,
    pub extension: String,
    pub operation: u32,
    pub task_id: u64,
    pub consistent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub extension: String,
    pub id: u64,
    pub name: String,
    pub state: String,
}

/// Original Linux siginfo metadata, before the dump writer interprets its layout.
/// A missing address is not a null address: the signal/code may not define one.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinuxSignalContext {
    pub signal: i32,
    pub code: i32,
    pub errno: i32,
    pub address: Option<LinuxSignalAddress>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinuxSignalAddress {
    pub address: u64,
    pub kind: LinuxSignalAddressKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinuxSignalAddressKind {
    MemoryAccess,
    Instruction,
}
