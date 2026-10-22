//! Out-of-process fatal diagnostics. Native faults always remain fatal.
pub(crate) mod model;
mod report;
mod supervisor;

pub use supervisor::{
    ContextGuard, CrashGuard, context, install, install_panic_hook, maybe_monitor, record_panic,
    record_task, register_thread,
};
