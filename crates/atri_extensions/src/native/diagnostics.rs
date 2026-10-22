use atri_core::abi::{HostApi, OwnedBuffer, ThreadActivity};
use std::{
    cell::Cell,
    future::Future,
    io::Write,
    sync::{Once, atomic::Ordering},
};

thread_local! {
    static SLOT: Cell<*const ThreadActivity> = const { Cell::new(std::ptr::null()) };
    static HOST: Cell<Option<HostApi>> = const { Cell::new(None) };
    static IN_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn os_thread_id() -> u64 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::syscall(libc::SYS_gettid) as u64 }
    }
    #[cfg(target_os = "macos")]
    {
        let mut id = 0;
        if unsafe { libc::pthread_threadid_np(0, &mut id) } == 0 {
            id
        } else {
            0
        }
    }
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() as u64 }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        0
    }
}

pub(super) fn register(host: HostApi) {
    install_panic_hook();
    let id = os_thread_id();
    let slot = if id == 0 {
        std::ptr::null()
    } else {
        unsafe { (host.register_thread)(host.context, id) }
    };
    SLOT.with(|value| value.set(slot));
    HOST.with(|value| value.set(Some(host)));
}

pub(super) fn exit() {
    let slot = SLOT.with(|value| value.replace(std::ptr::null()));
    if !slot.is_null() {
        update(unsafe { &*slot }, 255, 0);
    }
    HOST.with(|value| value.set(None));
}

pub(super) struct ThreadGuard;
impl ThreadGuard {
    pub(super) fn enter(host: HostApi) -> Self {
        register(host);
        Self
    }
}
impl Drop for ThreadGuard {
    fn drop(&mut self) {
        exit();
    }
}

fn update(slot: &ThreadActivity, operation: u32, task_id: u64) {
    // The one executor thread is the only writer. Odd sequence denotes an
    // interrupted update; a fatal reader must never spin waiting for that writer.
    slot.sequence.fetch_add(1, Ordering::AcqRel);
    slot.operation.store(operation, Ordering::Relaxed);
    slot.task_id.store(task_id, Ordering::Relaxed);
    slot.sequence.fetch_add(1, Ordering::Release);
}

struct ActivityGuard {
    slot: *const ThreadActivity,
    operation: u32,
    task_id: u64,
}
impl ActivityGuard {
    fn enter(operation: u32, task_id: u64) -> Self {
        let slot = SLOT.with(Cell::get);
        if slot.is_null() {
            return Self {
                slot,
                operation: 0,
                task_id: 0,
            };
        }
        let state = unsafe { &*slot };
        let guard = Self {
            slot,
            operation: state.operation.load(Ordering::Relaxed),
            task_id: state.task_id.load(Ordering::Relaxed),
        };
        update(state, operation, task_id);
        guard
    }
}
impl Drop for ActivityGuard {
    fn drop(&mut self) {
        if !self.slot.is_null() {
            update(unsafe { &*self.slot }, self.operation, self.task_id);
        }
    }
}

/// Guards each poll, not the lifetime of the future: unrelated async jobs on the
/// same executor thread must never inherit a suspended job's diagnostic identity.
pub(super) async fn track<F: Future>(operation: u32, task_id: u64, future: F) -> F::Output {
    futures::pin_mut!(future);
    futures::future::poll_fn(|cx| {
        let _activity = ActivityGuard::enter(operation, task_id);
        future.as_mut().poll(cx)
    })
    .await
}

pub(super) fn during<T>(operation: u32, task_id: u64, body: impl FnOnce() -> T) -> T {
    let _activity = ActivityGuard::enter(operation, task_id);
    body()
}

fn install_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let host = HOST.try_with(Cell::get).ok().flatten();
            let Some(host) = host else {
                previous(info);
                return;
            };
            if IN_PANIC_HOOK
                .try_with(|value| value.replace(true))
                .unwrap_or(true)
            {
                return;
            }
            let thread = std::thread::current();
            let trace = std::backtrace::Backtrace::force_capture();
            let mut report = format!(
                "panic in native extension executor\nthread: {} (OS tid {})\n{info}\n{trace}\n",
                thread.name().unwrap_or("unnamed"),
                os_thread_id()
            );
            // Panic diagnostics are exceptional, but still bounded before crossing ABI.
            const LIMIT: usize = 1024 * 1024;
            if report.len() > LIMIT {
                let mut end = LIMIT;
                while !report.is_char_boundary(end) {
                    end -= 1;
                }
                report.truncate(end);
                report.push_str("\n[backtrace truncated]\n");
            }
            let _ = std::io::stderr().lock().write_all(report.as_bytes());
            unsafe {
                (host.report_panic)(
                    host.context,
                    os_thread_id(),
                    OwnedBuffer::from_vec(report.into_bytes()),
                )
            };
            let _ = IN_PANIC_HOOK.try_with(|value| value.set(false));
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll};

    struct ResetSlot;
    impl Drop for ResetSlot {
        fn drop(&mut self) {
            SLOT.with(|value| value.set(std::ptr::null()));
        }
    }

    #[test]
    fn suspended_task_restores_enclosing_thread_identity_on_every_poll() {
        let slot = ThreadActivity::default();
        SLOT.with(|value| value.set(&slot));
        let _reset = ResetSlot;
        update(&slot, 3, 11);
        let mut polls = 0;
        let child = futures::future::poll_fn(|_| {
            assert_eq!(slot.operation.load(Ordering::Acquire), 5);
            assert_eq!(slot.task_id.load(Ordering::Acquire), u64::MAX);
            polls += 1;
            if polls == 1 {
                Poll::Pending
            } else {
                Poll::Ready(42)
            }
        });
        let mut tracked = Box::pin(track(5, u64::MAX, child));
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(tracked.as_mut().poll(&mut cx).is_pending());
        assert_eq!(slot.operation.load(Ordering::Acquire), 3);
        assert_eq!(slot.task_id.load(Ordering::Acquire), 11);
        assert_eq!(tracked.as_mut().poll(&mut cx), Poll::Ready(42));
        assert_eq!(slot.operation.load(Ordering::Acquire), 3);
        assert_eq!(slot.task_id.load(Ordering::Acquire), 11);
        assert_eq!(slot.sequence.load(Ordering::Acquire) % 2, 0);
    }

    #[test]
    fn panic_does_not_leave_the_thread_attributed_to_the_failed_task() {
        let slot = ThreadActivity::default();
        SLOT.with(|value| value.set(&slot));
        let _reset = ResetSlot;
        update(&slot, 1, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on(track(5, u64::MAX, async {
                assert_eq!(slot.operation.load(Ordering::Acquire), 5);
                panic!("isolated diagnostic scope regression");
            }));
        }));
        assert!(result.is_err());
        assert_eq!(slot.operation.load(Ordering::Acquire), 1);
        assert_eq!(slot.task_id.load(Ordering::Acquire), 0);
        assert_eq!(slot.sequence.load(Ordering::Acquire) % 2, 0);
    }
}
