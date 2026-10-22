use atri_core::{ApiError, TaskRecord};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub(crate) struct Tasks {
    next: Arc<AtomicU64>,
    entries: Arc<Mutex<BTreeMap<u64, Entry>>>,
}
struct Entry {
    owner: String,
    record: TaskRecord,
    cancel: Option<CancellationToken>,
}
pub(crate) struct TaskGuard {
    tasks: Tasks,
    id: u64,
}
impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.tasks.finish(self.id);
    }
}
impl Tasks {
    pub fn begin(&self, owner: &str, name: &str, cancel: CancellationToken) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let record = TaskRecord {
            id,
            name: name.into(),
            state: "running".into(),
        };
        crate::crash::record_task(owner, &record);
        self.entries.lock().unwrap().insert(
            id,
            Entry {
                owner: owner.into(),
                record,
                cancel: Some(cancel),
            },
        );
        id
    }
    pub fn finish(&self, id: u64) {
        if let Some(mut entry) = self.entries.lock().unwrap().remove(&id) {
            // A drop guard cannot distinguish success, cancellation and unwinding.
            entry.record.state = "finished".into();
            crate::crash::record_task(&entry.owner, &entry.record);
        }
    }
    pub fn finish_on_drop(&self, id: u64) -> TaskGuard {
        TaskGuard {
            tasks: self.clone(),
            id,
        }
    }
    pub fn native(&self, owner: &str, record: TaskRecord, finished: bool) -> Result<(), ApiError> {
        if record.id < (1 << 63) {
            return Err(ApiError::new(
                "invalid_task",
                "native task ids must have the high bit set",
            ));
        }
        let mut entries = self.entries.lock().unwrap();
        if let Some(existing) = entries.get(&record.id) {
            if existing.owner != owner {
                return Err(ApiError::new(
                    "task_conflict",
                    "task belongs to another extension",
                ));
            }
        }
        crate::crash::record_task(owner, &record);
        if finished {
            entries.remove(&record.id);
        } else {
            entries.insert(
                record.id,
                Entry {
                    owner: owner.into(),
                    record,
                    cancel: None,
                },
            );
        }
        Ok(())
    }
    pub fn list(&self, owner: Option<&str>) -> Vec<TaskRecord> {
        self.entries
            .lock()
            .unwrap()
            .values()
            .filter(|e| owner.is_none_or(|o| e.owner == o))
            .map(|e| {
                let mut record = e.record.clone();
                if owner.is_none() {
                    record.name = format!("{}: {}", e.owner, record.name);
                }
                record
            })
            .collect()
    }
    pub fn cancel(&self, id: u64) -> Result<Option<String>, ApiError> {
        let entries = self.entries.lock().unwrap();
        let entry = entries
            .get(&id)
            .ok_or_else(|| ApiError::new("task_not_found", id.to_string()))?;
        if let Some(token) = &entry.cancel {
            token.cancel();
            Ok(None)
        } else {
            Ok(Some(entry.owner.clone()))
        }
    }
    pub fn clear(&self, owner: &str) {
        self.entries.lock().unwrap().retain(|_, e| e.owner != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_task_principal_is_bound_and_host_cancellation_is_scoped() {
        let tasks = Tasks::default();
        let token = CancellationToken::new();
        let id = tasks.begin("one", "sleep", token.clone());
        assert!(tasks.cancel(id).unwrap().is_none());
        assert!(token.is_cancelled());
        let native = TaskRecord {
            id: (1 << 63) | 1,
            name: "network".into(),
            state: "running".into(),
        };
        tasks.native("one", native.clone(), false).unwrap();
        assert!(tasks.native("two", native.clone(), true).is_err());
        assert_eq!(tasks.cancel(native.id).unwrap().as_deref(), Some("one"));
        tasks.clear("one");
        assert!(tasks.list(None).is_empty());
    }
}
