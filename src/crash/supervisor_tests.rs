use super::*;

fn slot(sequence: u64, operation: u32, task_id: u64) -> Slot {
    let mut extension = [0u8; MAX_EXTENSION];
    let name = b"io.atri.probe.crash";
    extension[..name.len()].copy_from_slice(name);
    let activity = ThreadActivity::default();
    activity.sequence.store(sequence, Ordering::Relaxed);
    activity.operation.store(operation, Ordering::Relaxed);
    activity.task_id.store(task_id, Ordering::Relaxed);
    Slot {
        tid: 0x1234_5678_90ab,
        extension,
        extension_len: name.len(),
        activity,
    }
}

#[test]
fn fatal_wire_preserves_exact_source_and_full_width_native_task_id() {
    let slot = slot(2, 5, (1u64 << 63) + 97);
    let mut buffer = [0u8; THREAD_HEADER + MAX_EXTENSION];
    let len = encode_thread(&slot, &mut buffer);
    let decoded = decode_thread(&buffer[..len]).expect("valid fixed record");
    assert_eq!(decoded.os_thread_id, slot.tid);
    assert_eq!(decoded.extension, "io.atri.probe.crash");
    assert_eq!(decoded.operation, 5);
    assert_eq!(decoded.task_id, (1u64 << 63) + 97);
    assert!(decoded.consistent);
}

#[test]
fn interrupted_seqlock_writer_is_reported_as_inconsistent_without_spinning() {
    let slot = slot(3, 5, 27);
    let mut buffer = [0u8; THREAD_HEADER + MAX_EXTENSION];
    let len = encode_thread(&slot, &mut buffer);
    assert!(
        !decode_thread(&buffer[..len])
            .expect("valid fixed record")
            .consistent
    );
}

#[test]
fn fatal_wire_rejects_invalid_identity_and_oversized_payload() {
    assert!(decode_thread(&[0u8; THREAD_HEADER]).is_none());
    assert!(decode_thread(&[0u8; THREAD_HEADER + MAX_EXTENSION + 1]).is_none());
    let mut buffer = [0u8; THREAD_HEADER + 1];
    buffer[THREAD_HEADER] = 0xff;
    assert!(decode_thread(&buffer).is_none());
}

#[test]
fn normal_context_text_limits_preserve_utf8_and_disclose_truncation() {
    let value = "🦀".repeat(2000);
    let result = bounded(&value, 1024);
    assert!(result.len() <= 1024);
    assert!(result.ends_with("[diagnostic text truncated]"));
    assert_eq!(bounded("unchanged", 32), "unchanged");
}

fn monitor_handler() -> MonitorHandler {
    MonitorHandler {
        directory: PathBuf::new(),
        nonce: "capability".into(),
        pid: 7,
        terminal: None,
        authenticated: AtomicBool::new(false),
        connected: std::sync::Arc::new(AtomicBool::new(false)),
        shutdown: std::sync::Arc::new(AtomicBool::new(false)),
        pending_dump: Mutex::new(None),
        signal_context: Mutex::new(None),
        snapshot: Mutex::new(CrashSnapshot {
            version: 1,
            pid: 7,
            executable: "atri".into(),
            threads: Vec::new(),
            tasks: Vec::new(),
        }),
    }
}

fn authenticate(handler: &MonitorHandler) {
    let hello = Hello {
        nonce: "capability".into(),
        snapshot: CrashSnapshot {
            version: 1,
            pid: 7,
            executable: "atri".into(),
            threads: Vec::new(),
            tasks: Vec::new(),
        },
    };
    handler.on_message(
        HELLO,
        serde_json::to_vec(&hello).expect("serializable hello"),
    );
}

#[test]
fn monitor_rejects_unauthenticated_messages_and_minidump_creation() {
    let handler = monitor_handler();
    handler.on_message(TASK, b"{}".to_vec());
    assert!(handler.shutdown.load(Ordering::Acquire));
    assert_eq!(
        handler
            .create_minidump_file()
            .expect_err("must require capability")
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
}

#[test]
fn monitor_retains_exact_task_updates_and_multiple_extension_sources_per_tid() {
    let handler = monitor_handler();
    authenticate(&handler);
    assert!(handler.authenticated.load(Ordering::Acquire));
    let task = TaskSnapshot {
        extension: "io.atri.probe.crash".into(),
        id: 0x8000_0000_0000_0001,
        name: "native fault task".into(),
        state: "running".into(),
    };
    handler.on_message(TASK, serde_json::to_vec(&task).expect("serializable task"));
    let changed = TaskSnapshot {
        state: "finished".into(),
        ..task
    };
    handler.on_message(
        TASK,
        serde_json::to_vec(&changed).expect("serializable task"),
    );
    let mut one = slot(4, 5, changed.id);
    let mut buffer = [0u8; THREAD_HEADER + MAX_EXTENSION];
    let len = encode_thread(&one, &mut buffer);
    handler.on_message(THREAD, buffer[..len].to_vec());
    one.extension[..4].copy_from_slice(b"host");
    one.extension_len = 4;
    let len = encode_thread(&one, &mut buffer);
    handler.on_message(THREAD, buffer[..len].to_vec());
    let snapshot = handler.snapshot.lock().expect("snapshot lock");
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].id, changed.id);
    assert_eq!(snapshot.tasks[0].state, "finished");
    assert_eq!(snapshot.threads.len(), 2);
    assert_eq!(
        snapshot.threads[0].os_thread_id,
        snapshot.threads[1].os_thread_id
    );
}

#[test]
fn timed_out_history_does_not_hide_new_live_tasks_when_registry_is_full() {
    let handler = monitor_handler();
    authenticate(&handler);
    {
        let mut snapshot = handler.snapshot.lock().unwrap();
        snapshot.tasks = (0..MAX_TASKS)
            .map(|id| TaskSnapshot {
                extension: "example.deadlines".into(),
                id: id as u64,
                name: "expired request".into(),
                state: "timeout".into(),
            })
            .collect();
    }
    let live = TaskSnapshot {
        extension: "example.live".into(),
        id: u64::MAX,
        name: "current connection".into(),
        state: "running".into(),
    };
    handler.on_message(TASK, serde_json::to_vec(&live).unwrap());
    let snapshot = handler.snapshot.lock().unwrap();
    assert!(
        snapshot
            .tasks
            .iter()
            .any(|task| task.extension == live.extension
                && task.id == live.id
                && task.state == "running")
    );
    assert!(
        !snapshot
            .tasks
            .iter()
            .any(|task| task.extension == "example.deadlines" && task.id == 0)
    );
}

#[test]
fn original_signal_wire_distinguishes_nonnull_null_missing_and_instruction_addresses() {
    for address in [
        Some(LinuxSignalAddress {
            address: 0x7f4a33def000,
            kind: LinuxSignalAddressKind::MemoryAccess,
        }),
        Some(LinuxSignalAddress {
            address: 0,
            kind: LinuxSignalAddressKind::MemoryAccess,
        }),
        Some(LinuxSignalAddress {
            address: 0x1234,
            kind: LinuxSignalAddressKind::Instruction,
        }),
        None,
    ] {
        let context = LinuxSignalContext {
            signal: 11,
            code: 2,
            errno: -17,
            address,
        };
        let encoded = encode_linux_signal(&context);
        assert_eq!(decode_linux_signal(&encoded), Some(context));
        let handler = monitor_handler();
        authenticate(&handler);
        handler.on_message(LINUX_SIGNAL, encoded.to_vec());
        assert_eq!(*handler.signal_context.lock().unwrap(), Some(context));
        assert!(!handler.shutdown.load(Ordering::Acquire));
    }
}

#[test]
fn malformed_original_signal_metadata_is_not_silently_accepted() {
    let encoded = encode_linux_signal(&LinuxSignalContext {
        signal: 11,
        code: 128,
        errno: 0,
        address: None,
    });
    assert!(decode_linux_signal(&encoded[..SIGNAL_MESSAGE_BYTES - 1]).is_none());
    assert!(decode_linux_signal(&[0; SIGNAL_MESSAGE_BYTES + 1]).is_none());
    for (offset, value) in [(0, 2), (13, 3), (14, 1)] {
        let mut bad = encoded;
        bad[offset] = value;
        assert!(decode_linux_signal(&bad).is_none());
        let handler = monitor_handler();
        authenticate(&handler);
        handler.on_message(LINUX_SIGNAL, bad.to_vec());
        assert!(handler.shutdown.load(Ordering::Acquire));
        assert!(handler.signal_context.lock().unwrap().is_none());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn original_signal_address_distinguishes_protected_null_and_user_signals() {
    use std::os::{fd::AsRawFd, unix::process::ExitStatusExt};
    const CHILD_PATH: &str = "ATRI_TEST_ORIGINAL_SIGINFO_PATH";
    const CHILD_KIND: &str = "ATRI_TEST_ORIGINAL_SIGINFO_KIND";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        // Run only in a dedicated subprocess: every actual fault remains fatal.
        let mut file = crate::storage::create_private_file(Path::new(&path)).unwrap();
        unsafe {
            let no_core = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            assert_eq!(libc::setrlimit(libc::RLIMIT_CORE, &no_core), 0);
        }
        let kind = std::env::var(CHILD_KIND).unwrap();
        let address = if kind == "protected" {
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            assert!(page_size > 0);
            let address = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    page_size as usize,
                    libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            assert_ne!(address, libc::MAP_FAILED);
            assert!(!address.is_null());
            address
        } else {
            ptr::null_mut()
        };
        file.write_all(&(address as usize as u64).to_le_bytes())
            .unwrap();
        let fd = file.as_raw_fd();
        let _handler = crash_handler::CrashHandler::attach(unsafe {
            crash_handler::make_crash_event(move |context| {
                let encoded = encode_linux_signal(&linux_signal_context(context));
                // Fatal-side code has only fixed stack storage, copying, and the
                // async-signal-safe OS write. No panic, allocation or recovery.
                libc::write(fd, encoded.as_ptr().cast(), encoded.len());
                crash_handler::CrashEventResult::Handled(true)
            })
        })
        .unwrap();
        // Real writes must fault; an explicitly raised signal must also remain
        // fatal, but its siginfo union must not become a made-up memory address.
        unsafe {
            if kind == "user" {
                libc::raise(libc::SIGSEGV);
            } else {
                libc::memset(std::hint::black_box(address), 0x7f, 1);
            }
        }
        std::process::exit(91);
    }
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let directory =
        Directory(std::env::temp_dir().join(format!("atri-siginfo-{}", uuid::Uuid::new_v4())));
    crate::storage::private_directory(&directory.0).unwrap();
    let module = module_path!().split_once("::").unwrap().1;
    let test_name =
        format!("{module}::original_signal_address_distinguishes_protected_null_and_user_signals");
    for kind in ["protected", "null", "user"] {
        let path = directory.0.join(kind);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test_name, "--nocapture"])
            .env(CHILD_PATH, &path)
            .env(CHILD_KIND, kind)
            .output()
            .unwrap();
        assert_eq!(
            output.status.signal(),
            Some(libc::SIGSEGV),
            "fatal child must terminate by SIGSEGV; stdout={:?} stderr={:?}",
            output.stdout,
            output.stderr
        );
        let bytes = fs::read(path).unwrap();
        assert_eq!(bytes.len(), 8 + SIGNAL_MESSAGE_BYTES);
        let expected = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        assert_eq!(expected == 0, kind != "protected");
        let signal = decode_linux_signal(&bytes[8..]).unwrap();
        assert_eq!(signal.signal, libc::SIGSEGV);
        if kind == "user" {
            assert!(signal.code <= 0);
            assert!(signal.address.is_none());
        } else {
            assert_eq!(signal.code, if kind == "protected" { 2 } else { 1 });
            assert_eq!(
                signal.address,
                Some(LinuxSignalAddress {
                    address: expected,
                    kind: LinuxSignalAddressKind::MemoryAccess
                })
            );
        }
    }
}
