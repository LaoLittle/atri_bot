use super::model::{
    CrashSnapshot, LinuxSignalAddress, LinuxSignalAddressKind, LinuxSignalContext, TaskSnapshot,
    ThreadSnapshot,
};
use atri_core::{ApiError, TaskRecord, abi::ThreadActivity};
use minidumper::{Client, LoopAction, Server, ServerHandler, SocketName};
use serde::{Deserialize, Serialize};
use std::{
    backtrace::Backtrace,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, ExitCode, Stdio},
    ptr,
    rc::Rc,
    sync::{
        Mutex, Once, OnceLock,
        atomic::{AtomicBool, AtomicPtr, Ordering},
    },
    time::{Duration, Instant},
};

const MONITOR_ARG: &str = "--__atri-crash-monitor";
const MAX_THREADS: usize = 1024;
const MAX_EXTENSION: usize = 256;
const MAX_TASKS: usize = 4096;
const MAX_MESSAGE: usize = 48 * 1024;
const HELLO: u32 = 1;
const THREAD: u32 = 2;
const TASK: u32 = 3;
const PANIC: u32 = 4;
const LINUX_SIGNAL: u32 = 5;
// version:u8, signal/code/errno:i32, address-kind:u8, address:u64.
const SIGNAL_MESSAGE_BYTES: usize = 22;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
// tid:u64, operation:u32, task:u64, consistency:u8, identity bytes.
const THREAD_HEADER: usize = 21;
static ACTIVE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static STATE: OnceLock<State> = OnceLock::new();
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

struct Slot {
    tid: u64,
    extension: [u8; MAX_EXTENSION],
    extension_len: usize,
    activity: ThreadActivity,
}

struct State {
    client: Client,
    slots: [AtomicPtr<Slot>; MAX_THREADS],
    // Boxes keep the C ABI slots stable even as their owner vector grows.
    registration: Mutex<Vec<Box<Slot>>>,
    crashing: AtomicBool,
    warned: AtomicBool,
}

/// Keeps exception handlers and their owned helper alive. Drop before normal exit.
/// Diagnostic slots themselves deliberately live until process termination, since
/// independently loaded native modules retain their C ABI pointers.
pub struct CrashGuard {
    handler: Option<crash_handler::CrashHandler>,
    state: &'static State,
    _monitor: OwnedMonitor,
}

struct OwnedMonitor {
    child: Child,
    input: Option<ChildStdin>,
    ipc_dir: PathBuf,
}

impl Drop for OwnedMonitor {
    fn drop(&mut self) {
        // EOF is an OS-owned parent-liveness signal, including abrupt parent exit.
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        cleanup_ipc(&self.ipc_dir);
    }
}

impl Drop for CrashGuard {
    fn drop(&mut self) {
        let _ = ACTIVE.compare_exchange(
            self.state as *const State as *mut State,
            ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // Detach first, so no new fatal callback can begin after monitor shutdown.
        drop(self.handler.take());
        // Drain ordinary panic/task messages before EOF can stop the helper,
        // but never let an unresponsive helper hang normal process shutdown.
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let state = self.state;
        if std::thread::Builder::new()
            .name("atri-crash-drain".into())
            .spawn(move || {
                let _ = tx.send(state.client.ping());
            })
            .is_ok()
        {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
        #[cfg(target_os = "linux")]
        unsafe {
            libc::prctl(libc::PR_SET_PTRACER, 0, 0, 0, 0);
        }
        // monitor's Drop closes the liveness pipe, waits and reaps (or kills).
    }
}

#[derive(Serialize, Deserialize)]
struct Hello {
    nonce: String,
    snapshot: CrashSnapshot,
}
#[derive(Serialize, Deserialize)]
struct PanicRecord {
    extension: String,
    tid: u64,
    report: String,
}

fn error(value: impl std::fmt::Display) -> ApiError {
    ApiError::new("crash_diagnostics", value.to_string())
}

/// Must run before CLI parsing, Tokio creation, configuration or extension loading.
pub fn maybe_monitor() -> Option<ExitCode> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != MONITOR_ARG) {
        return None;
    }
    let result = (|| {
        if args.len() != 6 {
            return Err(error("invalid internal crash monitor arguments"));
        }
        let pid = args[2]
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|pid| *pid != 0)
            .ok_or_else(|| error("invalid crash monitor parent PID"))?;
        let nonce = args[3]
            .to_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| error("invalid crash monitor capability"))?
            .simple()
            .to_string();
        let ipc = PathBuf::from(&args[4]);
        let bot = PathBuf::from(&args[5]);
        let expected_directory = format!("atri-crash-{nonce}");
        if !ipc.is_absolute()
            || !bot.is_absolute()
            || ipc.file_name().and_then(|s| s.to_str()) != Some(expected_directory.as_str())
        {
            return Err(error("invalid crash monitor paths"));
        }
        #[cfg(unix)]
        if unsafe { libc::getppid() } as u32 != pid {
            return Err(error("crash monitor parent identity does not match"));
        }
        run_monitor(pid, nonce, ipc, bot)
    })();
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("atri crash monitor: {failure}");
            ExitCode::FAILURE
        }
    })
}

pub fn install(bot_dir: &Path) -> Result<CrashGuard, ApiError> {
    let _installation = INSTALL_LOCK
        .lock()
        .map_err(|_| error("crash installation lock poisoned"))?;
    if STATE.get().is_some() {
        return Err(error(
            "crash diagnostics can only be installed once per process",
        ));
    }
    crate::storage::crash_directory(bot_dir)?;
    let bot_dir = fs::canonicalize(bot_dir).map_err(error)?;
    let executable = std::env::current_exe().map_err(error)?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    // A short path matters on ALL three systems (AF_UNIX sun_path), including
    // Windows. The directory ACL/0700, not socket mode assumptions, is the boundary.
    // macOS's usual TMPDIR lives below symlinked /var and is often too long
    // for sun_path. Canonical /tmp is short, and the new child remains private.
    #[cfg(unix)]
    let ipc_root = fs::canonicalize("/tmp").map_err(error)?;
    #[cfg(windows)]
    let ipc_root = fs::canonicalize(std::env::temp_dir()).map_err(error)?;
    let ipc_dir = ipc_root.join(format!("atri-crash-{nonce}"));
    crate::storage::private_directory(&ipc_dir)?;
    let ipc_dir = fs::canonicalize(ipc_dir).map_err(error)?;
    let socket = ipc_dir.join("socket");
    if socket.to_str().is_none_or(|s| s.len() >= 104) {
        cleanup_ipc(&ipc_dir);
        return Err(error(
            "temporary directory path is too long or not UTF-8 for cross-platform crash IPC",
        ));
    }
    let child = Command::new(&executable)
        .arg(MONITOR_ARG)
        .arg(std::process::id().to_string())
        .arg(&nonce)
        .arg(&ipc_dir)
        .arg(&bot_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(failure) => {
            cleanup_ipc(&ipc_dir);
            return Err(error(failure));
        }
    };
    let input = child.stdin.take();
    let output = child
        .stdout
        .take()
        .ok_or_else(|| error("monitor readiness pipe missing"));
    let monitor = OwnedMonitor {
        child,
        input,
        ipc_dir,
    };
    if monitor.input.is_none() {
        return Err(error("monitor parent-liveness pipe missing"));
    }
    let output = output?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("atri-crash-readiness".into())
        .spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output)
                .take(256)
                .read_line(&mut line)
                .map(|_| line);
            let _ = tx.send(result);
        })
        .map_err(error)?;
    let line = rx
        .recv_timeout(STARTUP_TIMEOUT)
        .map_err(error)?
        .map_err(error)?;
    if line != format!("ATRI_CRASH_READY {nonce}\n") {
        return Err(error("monitor did not become ready"));
    }
    let hello = Hello {
        nonce,
        snapshot: CrashSnapshot {
            version: 1,
            pid: std::process::id(),
            executable: executable.to_string_lossy().into_owned(),
            threads: Vec::new(),
            tasks: Vec::new(),
        },
    };
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("atri-crash-handshake".into())
        .spawn(move || {
            let result = (|| {
                let client = Client::with_name(SocketName::Path(&socket)).map_err(error)?;
                client
                    .send_message(HELLO, serde_json::to_vec(&hello).map_err(error)?)
                    .map_err(error)?;
                client.ping().map_err(error)?;
                Ok::<_, ApiError>(client)
            })();
            let _ = tx.send(result);
        })
        .map_err(error)?;
    let client = rx.recv_timeout(STARTUP_TIMEOUT).map_err(error)??;
    // A process-global explicit owner retains every cross-DLL C ABI slot until
    // termination, including after logical extension disable or guard Drop.
    STATE
        .set(State {
            client,
            slots: std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut())),
            registration: Mutex::new(Vec::with_capacity(MAX_THREADS)),
            crashing: AtomicBool::new(false),
            warned: AtomicBool::new(false),
        })
        .map_err(|_| error("crash diagnostics already initialized"))?;
    let state = STATE
        .get()
        .ok_or_else(|| error("crash diagnostics state unavailable"))?;
    let fatal_state = state;
    // SAFETY: callback accesses only atomics and immutable process-owned memory, uses a
    // small fixed stack buffer, and vetted minidumper IPC plus literal stderr.
    // In particular: no formatting, allocator, ordinary mutex, logger or unwinder.
    let handler = crash_handler::CrashHandler::attach(unsafe {
        crash_handler::make_crash_event(move |crash_context| {
            if fatal_state.crashing.swap(true, Ordering::AcqRel) {
                return crash_handler::CrashEventResult::Handled(false);
            }
            crash_handler::write_stderr("\natri: fatal native fault; collecting private crash diagnostics in monitor\n");
            let mut buffer = [0u8; THREAD_HEADER + MAX_EXTENSION];
            for entry in &fatal_state.slots {
                let slot = entry.load(Ordering::Acquire);
                if slot.is_null() { continue; }
                let slot = &*slot;
                let length = encode_thread(slot, &mut buffer);
                if fatal_state.client.send_message(THREAD, &buffer[..length]).is_err() {
                    crash_handler::write_stderr("atri: crash thread context IPC failed\n");
                    break;
                }
            }
            #[cfg(target_os = "linux")]
            {
                let signal = linux_signal_context(crash_context);
                let metadata = encode_linux_signal(&signal);
                if fatal_state.client.send_message(LINUX_SIGNAL, &metadata).is_err() {
                    crash_handler::write_stderr("atri: original Linux signal context IPC failed; raw fault address may be unavailable\n");
                }
            }
            // Mach crash requests and AF_UNIX user messages use different
            // transports. PING is REQUIRED to flush annotations before the Mach
            // request, and is the only fatal-side receive before request_dump.
            if fatal_state.client.ping().is_err() {
                crash_handler::write_stderr("atri: crash context flush failed\n");
            }
            let captured = fatal_state.client.request_dump(crash_context).is_ok();
            if !captured { crash_handler::write_stderr("atri: monitor could not capture native minidump\n"); }
            #[cfg(windows)]
            {
                // The crate's CRT purecall path otherwise exits with SUCCESS
                // for Handled(true). No hooked fatal exception may do that.
                terminate_windows(crash_context.exception_code as u32);
            }
            #[cfg(target_os = "macos")]
            { crash_handler::CrashEventResult::Handled(false) }
            #[cfg(target_os = "linux")]
            { crash_handler::CrashEventResult::Handled(captured) }
        })
    }).map_err(error)?;
    #[cfg(target_os = "linux")]
    {
        // Never use the library's default PR_SET_PTRACER_ANY. Set the selected
        // helper before exposing the handler to native extension code.
        handler.set_ptracer(Some(monitor.child.id()));
        let result = unsafe {
            libc::prctl(
                libc::PR_SET_PTRACER,
                monitor.child.id() as libc::c_ulong,
                0,
                0,
                0,
            )
        };
        if result != 0 {
            return Err(error(std::io::Error::last_os_error()));
        }
        // Keep the vetted pthread_create interposer linked: future native and
        // Tokio threads receive alternate signal stacks, not just this thread.
        std::hint::black_box(crash_handler::unix::pthread_create as *const () as usize);
    }
    ACTIVE.store(state as *const State as *mut State, Ordering::Release);
    Ok(CrashGuard {
        handler: Some(handler),
        state,
        _monitor: monitor,
    })
}

fn encode_thread(slot: &Slot, output: &mut [u8; THREAD_HEADER + MAX_EXTENSION]) -> usize {
    let before = slot.activity.sequence.load(Ordering::Acquire);
    let operation = slot.activity.operation.load(Ordering::Acquire);
    let task = slot.activity.task_id.load(Ordering::Acquire);
    let after = slot.activity.sequence.load(Ordering::Acquire);
    output[..8].copy_from_slice(&slot.tid.to_le_bytes());
    output[8..12].copy_from_slice(&operation.to_le_bytes());
    output[12..20].copy_from_slice(&task.to_le_bytes());
    output[20] = u8::from(before == after && before & 1 == 0);
    output[THREAD_HEADER..THREAD_HEADER + slot.extension_len]
        .copy_from_slice(&slot.extension[..slot.extension_len]);
    THREAD_HEADER + slot.extension_len
}

#[cfg(target_os = "linux")]
fn linux_signal_context(context: &crash_handler::CrashContext) -> LinuxSignalContext {
    // crash-handler 0.8.1 linux/state.rs copies a whole libc siginfo_t into its
    // signalfd_siginfo field without conversion; these layouts have DIFFERENT
    // address offsets. minidumper 0.11 forwards those bytes unchanged, and
    // minidump-writer 0.13 incorrectly reads ssi_addr. Recover the retained
    // original with libc's target-specific accessor, never a guessed byte offset.
    // Keep this tied to that pinned producer; do not reinterpret real signalfd data.
    const {
        assert!(
            std::mem::size_of::<libc::siginfo_t>() <= std::mem::size_of::<libc::signalfd_siginfo>()
        );
    }
    // SAFETY: the pinned handler supplies an initialized, byte-copied siginfo_t;
    // read_unaligned also covers targets with different struct alignment.
    let info = unsafe {
        ptr::read_unaligned(
            (&context.siginfo as *const libc::signalfd_siginfo).cast::<libc::siginfo_t>(),
        )
    };
    let kind = match (info.si_signo, info.si_code) {
        (libc::SIGSEGV, 1..=4 | 9) | (libc::SIGBUS, 1..=5) => {
            Some(LinuxSignalAddressKind::MemoryAccess)
        }
        (libc::SIGILL | libc::SIGFPE, 1..=8) | (libc::SIGTRAP, 1..=2) => {
            Some(LinuxSignalAddressKind::Instruction)
        }
        // SI_USER/SI_TKILL and SI_KERNEL do not define a memory-fault address;
        // unknown/new codes must not turn a zero-filled union into a null fault.
        _ => None,
    };
    LinuxSignalContext {
        signal: info.si_signo,
        code: info.si_code,
        errno: info.si_errno,
        // SAFETY: only the synchronous signal/code variants above define si_addr.
        address: kind.map(|kind| LinuxSignalAddress {
            address: unsafe { info.si_addr() } as usize as u64,
            kind,
        }),
    }
}

fn encode_linux_signal(context: &LinuxSignalContext) -> [u8; SIGNAL_MESSAGE_BYTES] {
    let mut output = [0; SIGNAL_MESSAGE_BYTES];
    output[0] = 1;
    output[1..5].copy_from_slice(&context.signal.to_le_bytes());
    output[5..9].copy_from_slice(&context.code.to_le_bytes());
    output[9..13].copy_from_slice(&context.errno.to_le_bytes());
    if let Some(address) = context.address {
        output[13] = match address.kind {
            LinuxSignalAddressKind::MemoryAccess => 1,
            LinuxSignalAddressKind::Instruction => 2,
        };
        output[14..22].copy_from_slice(&address.address.to_le_bytes());
    }
    output
}

fn decode_linux_signal(buffer: &[u8]) -> Option<LinuxSignalContext> {
    if buffer.len() != SIGNAL_MESSAGE_BYTES || buffer[0] != 1 {
        return None;
    }
    let address = u64::from_le_bytes(buffer[14..22].try_into().ok()?);
    let address = match buffer[13] {
        0 if address == 0 => None,
        1 => Some(LinuxSignalAddress {
            address,
            kind: LinuxSignalAddressKind::MemoryAccess,
        }),
        2 => Some(LinuxSignalAddress {
            address,
            kind: LinuxSignalAddressKind::Instruction,
        }),
        _ => return None,
    };
    Some(LinuxSignalContext {
        signal: i32::from_le_bytes(buffer[1..5].try_into().ok()?),
        code: i32::from_le_bytes(buffer[5..9].try_into().ok()?),
        errno: i32::from_le_bytes(buffer[9..13].try_into().ok()?),
        address,
    })
}

fn active() -> Option<&'static State> {
    let state = ACTIVE.load(Ordering::Acquire);
    if state.is_null() {
        None
    } else {
        Some(unsafe { &*state })
    }
}

pub fn register_thread(extension: &str, tid: u64) -> *const ThreadActivity {
    let Some(state) = active() else {
        return ptr::null();
    };
    if extension.is_empty() || extension.len() > MAX_EXTENSION || tid == 0 {
        return ptr::null();
    }
    let Ok(mut owner) = state.registration.lock() else {
        return ptr::null();
    };
    let mut vacant = None;
    for entry in &state.slots {
        let slot = entry.load(Ordering::Acquire);
        if slot.is_null() {
            vacant.get_or_insert(entry);
            continue;
        }
        let slot = unsafe { &*slot };
        if slot.tid == tid && &slot.extension[..slot.extension_len] == extension.as_bytes() {
            return &slot.activity;
        }
    }
    let Some(entry) = vacant else {
        if !state.warned.swap(true, Ordering::Relaxed) {
            eprintln!(
                "atri: crash thread registry exhausted (1024 stable slots); further thread attribution unavailable"
            );
        }
        return ptr::null();
    };
    let mut identity = [0u8; MAX_EXTENSION];
    identity[..extension.len()].copy_from_slice(extension.as_bytes());
    let slot = Box::new(Slot {
        tid,
        extension: identity,
        extension_len: extension.len(),
        activity: ThreadActivity::default(),
    });
    let slot_ptr = &*slot as *const Slot as *mut Slot;
    let activity_ptr = &slot.activity as *const ThreadActivity;
    owner.push(slot);
    entry.store(slot_ptr, Ordering::Release);
    activity_ptr
}

pub fn record_task(extension: &str, record: &TaskRecord) {
    let Some(state) = active() else {
        return;
    };
    if extension.len() > MAX_EXTENSION {
        return;
    }
    let task = TaskSnapshot {
        extension: extension.into(),
        id: record.id,
        name: bounded(&record.name, 1024),
        state: bounded(&record.state, 128),
    };
    if let Ok(bytes) = serde_json::to_vec(&task) {
        if bytes.len() <= MAX_MESSAGE && state.client.send_message(TASK, bytes).is_err() {
            eprintln!("atri: unable to send managed task diagnostic context to crash monitor");
        }
    }
}

/// Ordinary panic reporting only; never called from a native fatal callback.
pub fn record_panic(extension: &str, tid: u64, report: &str) {
    let Some(state) = active() else {
        return;
    };
    let mut record = PanicRecord {
        extension: bounded(extension, MAX_EXTENSION),
        tid,
        report: bounded(report, 40 * 1024),
    };
    loop {
        let Ok(bytes) = serde_json::to_vec(&record) else {
            return;
        };
        // Escaping can expand text sixfold. Trim only if serialized bytes exceed
        // the IPC limit, not merely because a full backtrace contains newlines.
        if bytes.len() > MAX_MESSAGE {
            record.report = bounded(&record.report, record.report.len() / 2);
            continue;
        }
        if state.client.send_message(PANIC, bytes).is_err() {
            eprintln!("atri: unable to persist ordinary panic diagnostic in crash monitor");
        }
        break;
    }
}

#[cfg(windows)]
fn terminate_windows(code: u32) -> ! {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn TerminateProcess(process: *mut std::ffi::c_void, code: u32) -> i32;
        fn ExitProcess(code: u32) -> !;
    }
    let code = if code == 0 { 0xc0000409 } else { code };
    unsafe {
        TerminateProcess(GetCurrentProcess(), code);
        ExitProcess(code);
    }
}

fn bounded(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.into();
    }
    let mut end = limit.saturating_sub(32);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[diagnostic text truncated]", &value[..end])
}

pub fn install_panic_hook() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        // Do not chain the default hook: it would print the panic/backtrace a
        // second time. SDK dylibs install their own hook for their std instance.
        std::panic::set_hook(Box::new(|info| {
            let thread = std::thread::current();
            let tid = os_thread_id();
            let report = format!(
                "atri: Rust panic on host thread '{}' (OS TID {tid})\n{info}\n{}\n",
                thread.name().unwrap_or("unnamed"),
                Backtrace::force_capture()
            );
            eprint!("{report}");
            record_panic("atri.host", tid, &report);
        }));
    });
}

/// Synchronous thread-bound attribution. This guard is deliberately !Send;
/// async callers must create one per Future::poll, never carry it across await.
pub struct ContextGuard {
    slot: *const ThreadActivity,
    operation: u32,
    task_id: u64,
    _thread_bound: PhantomData<Rc<()>>,
}

pub fn context(extension: &str, operation: u32, task_id: u64) -> ContextGuard {
    let slot = register_thread(extension, os_thread_id());
    let mut guard = ContextGuard {
        slot,
        operation: 0,
        task_id: 0,
        _thread_bound: PhantomData,
    };
    if !slot.is_null() {
        let activity = unsafe { &*slot };
        guard.operation = activity.operation.load(Ordering::Relaxed);
        guard.task_id = activity.task_id.load(Ordering::Relaxed);
        update_activity(activity, operation, task_id);
    }
    guard
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        if !self.slot.is_null() {
            update_activity(unsafe { &*self.slot }, self.operation, self.task_id);
        }
    }
}
fn update_activity(activity: &ThreadActivity, operation: u32, task_id: u64) {
    activity.sequence.fetch_add(1, Ordering::AcqRel);
    activity.operation.store(operation, Ordering::Relaxed);
    activity.task_id.store(task_id, Ordering::Relaxed);
    activity.sequence.fetch_add(1, Ordering::Release);
}

fn os_thread_id() -> u64 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::syscall(libc::SYS_gettid) as u64 }
    }
    #[cfg(target_os = "macos")]
    {
        let mut tid = 0;
        unsafe {
            libc::pthread_threadid_np(0, &mut tid);
        }
        tid
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThreadId() -> u32;
        }
        unsafe { GetCurrentThreadId() as u64 }
    }
}

struct MonitorHandler {
    directory: PathBuf,
    nonce: String,
    pid: u32,
    terminal: Option<TerminalState>,
    authenticated: AtomicBool,
    connected: std::sync::Arc<AtomicBool>,
    shutdown: std::sync::Arc<AtomicBool>,
    snapshot: Mutex<CrashSnapshot>,
    pending_dump: Mutex<Option<PathBuf>>,
    signal_context: Mutex<Option<LinuxSignalContext>>,
}

impl ServerHandler for MonitorHandler {
    fn create_minidump_file(&self) -> std::io::Result<(File, PathBuf)> {
        if !self.authenticated.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "unauthenticated crash client",
            ));
        }
        let path = self.directory.join(format!(
            "{}-{}.dmp",
            self.pid,
            uuid::Uuid::new_v4().simple()
        ));
        let file = crate::storage::create_private_file(&path).map_err(|failure| {
            eprintln!(
                "atri: could not create private minidump {}: {failure}",
                path.display()
            );
            std::io::Error::other(failure)
        })?;
        if let Ok(mut pending) = self.pending_dump.lock() {
            *pending = Some(path.clone());
        }
        Ok((file, path))
    }

    fn on_minidump_created(
        &self,
        result: Result<minidumper::MinidumpBinary, minidumper::Error>,
    ) -> LoopAction {
        // The healthy helper, not the compromised host, restores the terminal.
        if let Some(terminal) = &self.terminal {
            terminal.restore();
        }
        eprint!("\x1b[0m\x1b[?25h\x1b[?1049l\n");
        let path = match result {
            Ok(mut dump) => {
                let _ = dump.file.flush();
                let _ = dump.file.sync_all();
                Some(dump.path)
            }
            Err(failure) => {
                eprintln!("atri: native minidump capture failed: {failure}");
                self.pending_dump.lock().ok().and_then(|path| path.clone())
            }
        };
        if let Some(path) = path {
            let snapshot = self.snapshot.lock().map(|value| value.clone());
            let signal_context = self.signal_context.lock().ok().and_then(|value| *value);
            match (
                snapshot,
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build(),
            ) {
                (Ok(snapshot), Ok(runtime)) => match runtime.block_on(super::report::write_report(
                    &path,
                    &snapshot,
                    signal_context.as_ref(),
                )) {
                    Ok(text) => match fs::read_to_string(&text) {
                        Ok(report) => eprint!("{report}"),
                        Err(failure) => eprintln!(
                            "atri: saved crash report {} could not be read: {failure}",
                            text.display()
                        ),
                    },
                    Err(failure) => eprintln!(
                        "atri: crash report failed: {failure}; sensitive raw minidump retained privately at {}",
                        path.display()
                    ),
                },
                _ => eprintln!(
                    "atri: report executor unavailable; sensitive raw minidump retained privately at {}",
                    path.display()
                ),
            }
        }
        LoopAction::Exit
    }

    fn on_message(&self, kind: u32, buffer: Vec<u8>) {
        if buffer.len() > MAX_MESSAGE {
            self.shutdown.store(true, Ordering::Release);
            return;
        }
        if kind == HELLO {
            let hello = serde_json::from_slice::<Hello>(&buffer);
            if let Ok(hello) = hello {
                if !self.authenticated.load(Ordering::Acquire)
                    && hello.nonce == self.nonce
                    && hello.snapshot.pid == self.pid
                    && hello.snapshot.version == 1
                    && hello.snapshot.threads.is_empty()
                    && hello.snapshot.tasks.is_empty()
                {
                    if let Ok(mut snapshot) = self.snapshot.lock() {
                        *snapshot = hello.snapshot;
                        self.authenticated.store(true, Ordering::Release);
                        return;
                    }
                }
            }
            self.shutdown.store(true, Ordering::Release);
            return;
        }
        if !self.authenticated.load(Ordering::Acquire) {
            self.shutdown.store(true, Ordering::Release);
            return;
        }
        match kind {
            LINUX_SIGNAL => {
                let Some(context) = decode_linux_signal(&buffer) else {
                    eprintln!(
                        "atri: malformed original Linux signal metadata; stopping crash monitor"
                    );
                    self.shutdown.store(true, Ordering::Release);
                    return;
                };
                if let Ok(mut signal_context) = self.signal_context.lock() {
                    *signal_context = Some(context);
                }
            }
            THREAD => {
                if let Some(thread) = decode_thread(&buffer) {
                    if let Ok(mut snapshot) = self.snapshot.lock() {
                        // More than one extension can run on the same OS thread;
                        // retain all source slots and their exact operation/task.
                        if let Some(existing) = snapshot.threads.iter_mut().find(|old| {
                            old.os_thread_id == thread.os_thread_id
                                && old.extension == thread.extension
                        }) {
                            *existing = thread;
                        } else if snapshot.threads.len() < MAX_THREADS {
                            snapshot.threads.push(thread);
                        }
                    }
                }
            }
            TASK => {
                if let Ok(task) = serde_json::from_slice::<TaskSnapshot>(&buffer) {
                    if let Ok(mut snapshot) = self.snapshot.lock() {
                        if let Some(existing) = snapshot
                            .tasks
                            .iter_mut()
                            .find(|old| old.extension == task.extension && old.id == task.id)
                        {
                            *existing = task;
                        } else if snapshot.tasks.len() < MAX_TASKS {
                            snapshot.tasks.push(task);
                        } else {
                            // Reclaim finished history before sacrificing live tasks.
                            if let Some(index) = snapshot.tasks.iter().position(|old| {
                                matches!(
                                    old.state.as_str(),
                                    "finished" | "cancelled" | "failed" | "completed" | "timeout"
                                )
                            }) {
                                snapshot.tasks.remove(index);
                                snapshot.tasks.push(task);
                            } else {
                                eprintln!(
                                    "atri: crash monitor managed-task registry full; additional task records omitted"
                                );
                            }
                        }
                    }
                }
            }
            PANIC => {
                if let Ok(panic) = serde_json::from_slice::<PanicRecord>(&buffer) {
                    let path = self.directory.join(format!(
                        "{}-{}.panic.txt",
                        self.pid,
                        uuid::Uuid::new_v4().simple()
                    ));
                    let text = format!(
                        "Ordinary Rust panic (not a native minidump)\nprocess: {}\nextension: {}\nOS thread: {}\n\n{}",
                        self.pid, panic.extension, panic.tid, panic.report
                    );
                    match crate::storage::write_private_file(&path, text.as_bytes()) {
                        Ok(()) => eprintln!(
                            "atri: ordinary panic diagnostic saved privately at {}",
                            path.display()
                        ),
                        Err(failure) => {
                            eprintln!("atri: could not save ordinary panic diagnostic: {failure}")
                        }
                    }
                }
            }
            _ => self.shutdown.store(true, Ordering::Release),
        }
    }
    fn on_client_connected(&self, clients: usize) -> LoopAction {
        self.connected.store(true, Ordering::Release);
        if clients == 1 {
            LoopAction::Continue
        } else {
            LoopAction::Exit
        }
    }
    fn on_client_disconnected(&self, clients: usize) -> LoopAction {
        if clients == 0 {
            LoopAction::Exit
        } else {
            LoopAction::Continue
        }
    }
}

fn decode_thread(buffer: &[u8]) -> Option<ThreadSnapshot> {
    if buffer.len() <= THREAD_HEADER || buffer.len() > THREAD_HEADER + MAX_EXTENSION {
        return None;
    }
    Some(ThreadSnapshot {
        os_thread_id: u64::from_le_bytes(buffer[..8].try_into().ok()?),
        operation: u32::from_le_bytes(buffer[8..12].try_into().ok()?),
        task_id: u64::from_le_bytes(buffer[12..20].try_into().ok()?),
        consistent: buffer[20] == 1,
        extension: std::str::from_utf8(&buffer[THREAD_HEADER..]).ok()?.into(),
    })
}

fn run_monitor(
    pid: u32,
    nonce: String,
    ipc_dir: PathBuf,
    bot_dir: PathBuf,
) -> Result<(), ApiError> {
    // Validate the already-created capability directory through the SAME storage
    // path/ACL policy as all private runtime artifacts, before binding a socket.
    crate::storage::private_directory(&ipc_dir)?;
    let directory = crate::storage::crash_directory(&bot_dir)?;
    let socket = ipc_dir.join("socket");
    let mut server = Server::with_name(SocketName::Path(&socket)).map_err(error)?;
    let shutdown = std::sync::Arc::new(AtomicBool::new(false));
    let connected = std::sync::Arc::new(AtomicBool::new(false));
    let parent_shutdown = shutdown.clone();
    std::thread::Builder::new()
        .name("atri-crash-parent".into())
        .spawn(move || {
            let mut byte = [0u8; 1];
            loop {
                match std::io::stdin().read(&mut byte) {
                    Ok(0) | Err(_) => {
                        parent_shutdown.store(true, Ordering::Release);
                        break;
                    }
                    Ok(_) => {}
                }
            }
        })
        .map_err(error)?;
    let startup_shutdown = shutdown.clone();
    let startup_connected = connected.clone();
    std::thread::Builder::new()
        .name("atri-crash-startup".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_TIMEOUT);
            if !startup_connected.load(Ordering::Acquire) {
                startup_shutdown.store(true, Ordering::Release);
            }
        })
        .map_err(error)?;
    let handler = MonitorHandler {
        directory,
        nonce: nonce.clone(),
        pid,
        authenticated: AtomicBool::new(false),
        connected,
        terminal: TerminalState::capture(),
        shutdown: shutdown.clone(),
        pending_dump: Mutex::new(None),
        signal_context: Mutex::new(None),
        snapshot: Mutex::new(CrashSnapshot {
            version: 1,
            pid,
            executable: String::new(),
            threads: Vec::new(),
            tasks: Vec::new(),
        }),
    };
    println!("ATRI_CRASH_READY {nonce}");
    std::io::stdout().flush().map_err(error)?;
    let result = server
        .run(Box::new(handler), &shutdown, None)
        .map_err(error);
    drop(server);
    cleanup_ipc(&ipc_dir);
    result
}

// The helper starts before the host enters its dashboard, so it can retain the
// original terminal mode locally. Merely printing ANSI escapes would leave the
// user's terminal in raw mode after a fatal TUI crash.
#[cfg(unix)]
struct TerminalState(libc::termios);
#[cfg(unix)]
impl TerminalState {
    fn capture() -> Option<Self> {
        let mut mode = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(2, &mut mode) } == 0 {
            Some(Self(mode))
        } else {
            None
        }
    }
    fn restore(&self) {
        unsafe {
            libc::tcsetattr(2, libc::TCSANOW, &self.0);
        }
    }
}

#[cfg(windows)]
struct TerminalState {
    input: Option<(File, u32)>,
    output: Option<u32>,
}
#[cfg(windows)]
impl TerminalState {
    fn capture() -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        let input = File::open("CONIN$")
            .ok()
            .and_then(|file| console_mode(file.as_raw_handle()).map(|mode| (file, mode)));
        let output = console_mode(std::io::stderr().as_raw_handle());
        if input.is_some() || output.is_some() {
            Some(Self { input, output })
        } else {
            None
        }
    }
    fn restore(&self) {
        use std::os::windows::io::AsRawHandle;
        if let Some((file, mode)) = &self.input {
            set_console_mode(file.as_raw_handle(), *mode);
        }
        if let Some(mode) = self.output {
            set_console_mode(std::io::stderr().as_raw_handle(), mode);
        }
    }
}
#[cfg(windows)]
fn console_mode(handle: *mut std::ffi::c_void) -> Option<u32> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    }
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } != 0 {
        Some(mode)
    } else {
        None
    }
}
#[cfg(windows)]
fn set_console_mode(handle: *mut std::ffi::c_void, mode: u32) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleMode(handle: *mut std::ffi::c_void, mode: u32) -> i32;
    }
    unsafe {
        SetConsoleMode(handle, mode);
    }
}

fn cleanup_ipc(directory: &Path) {
    let _ = fs::remove_file(directory.join("socket"));
    let _ = fs::remove_dir(directory);
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
