use crate::signal::{disable_raw_mode, post_print_fatal, pre_print_fatal, DlBacktrace};
use std::cell::RefCell;
use std::mem::MaybeUninit;
use std::ptr::null_mut;

pub fn init_crash_handler() {
    let mut act = Sigaction {
        __sigaction_u: SigactionU {
            __sa_sigaction: Some(handle),
        },
        sa_mask: 0,
        sa_flags: 0,
    };

    extern "C" {
        fn sigemptyset(arg1: *mut SigsetT) -> std::ffi::c_int;

        fn sigaction(
            arg1: std::ffi::c_int,
            arg2: *const Sigaction,
            arg3: *mut Sigaction,
        ) -> std::ffi::c_int;
    }

    unsafe {
        sigemptyset(&mut act.sa_mask);

        for sig in [SIGSEGV, SIGBUS, SIGABRT] {
            if sigaction(SIGSEGV, &act, null_mut()) != 0 {
                eprintln!("signal {} handler 注册失败", sig);
            }
        }
    }
}

macro_rules! exception_sig {
    ($($sig:expr => $name:ident),* $(,)?) => {
        $(const $name: std::ffi::c_int = $sig;)*

        #[allow(dead_code)]
        const fn sig_name(code: std::ffi::c_int) -> &'static str {
            match code {
                $($sig => stringify!($name),)*
                _ => "unknown"
            }
        }
    };
}

exception_sig! {
    6 => SIGABRT,
    10 => SIGBUS,
    11 => SIGSEGV,
}

unsafe extern "C" fn handle(
    sig: std::ffi::c_int,
    _info: *mut Siginfo,
    _addr: *mut std::ffi::c_void,
) {
    fn dl_get_name(addr: *const std::ffi::c_void) -> String {
        extern "C" {
            fn dladdr(arg1: *const std::ffi::c_void, arg2: *mut DlInfo) -> std::ffi::c_int;
        }

        let mut di = MaybeUninit::<DlInfo>::uninit();
        let di = unsafe {
            if dladdr(addr, di.as_mut_ptr()) == 0 {
                return String::from("unknown");
            }

            di.assume_init()
        };

        unsafe {
            std::ffi::CStr::from_ptr(di.dli_fname)
                .to_string_lossy()
                .into_owned()
        }
    }

    let enabled = pre_print_fatal();
    crate::signal::fatal_error_print();

    eprintln!("exception address: {:p}", (*_info).si_addr);
    eprintln!(
        "stack backtrace:\n{}",
        DlBacktrace {
            inner: backtrace::Backtrace::new(),
            fun: dl_get_name
        }
    );

    eprintln!("Something went wrong, signal: {sig}({})", sig_name(sig));
    post_print_fatal(enabled);

    match sig {
        SIGSEGV if crate::service::plugin::is_rec_enabled() => exception_jmp(sig),
        or => {
            disable_raw_mode();
            std::process::exit(or);
        },
    }
}

pub type DarwinPidT = i32;
pub type DarwinUidT = u32;

pub type DarwinSigsetT = u32;

pub type PidT = DarwinPidT;
pub type UidT = DarwinUidT;

pub type SigsetT = DarwinSigsetT;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Siginfo {
    pub si_signo: std::ffi::c_int,
    pub si_errno: std::ffi::c_int,
    pub si_code: std::ffi::c_int,
    pub si_pid: PidT,
    pub si_uid: UidT,
    pub si_status: std::ffi::c_int,
    pub si_addr: *mut std::ffi::c_void,
    pub si_value: Sigval,
    pub si_band: std::ffi::c_long,
    pub __pad: [std::ffi::c_ulong; 7usize],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Sigval {
    pub sival_int: std::ffi::c_int,
    pub sival_ptr: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union SigactionU {
    pub __sa_handler: Option<unsafe extern "C" fn(arg1: std::ffi::c_int)>,
    pub __sa_sigaction: Option<
        unsafe extern "C" fn(
            arg1: std::ffi::c_int,
            arg2: *mut Siginfo,
            arg3: *mut std::ffi::c_void,
        ),
    >,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Sigaction {
    pub __sigaction_u: SigactionU,
    pub sa_mask: SigsetT,
    pub sa_flags: std::ffi::c_int,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DlInfo {
    pub dli_fname: *const std::ffi::c_char,
    pub dli_fbase: *mut std::ffi::c_void,
    pub dli_sname: *const std::ffi::c_char,
    pub dli_saddr: *mut std::ffi::c_void,
}

const _JBLEN: usize = (14 + 8 + 2) * 2;
const _SIGJBLEN: usize = _JBLEN + 1;

type SigjmpBuf = [std::ffi::c_int; _SIGJBLEN];

thread_local! {
    static JMP_BUF: RefCell<Option<SigjmpBuf>> = RefCell::new(None);
}

pub unsafe fn save_jmp() {
    extern "C" {
        fn sigsetjmp(buf: *mut std::ffi::c_int, save_mask: std::ffi::c_int) -> std::ffi::c_int;
    }

    let mut buf: SigjmpBuf = [0; _SIGJBLEN];
    let ret = unsafe { sigsetjmp(buf.as_mut_ptr(), true as _) };

    if ret != 0 {
        panic!("exception occurred, status: {ret}");
    }

    JMP_BUF.with(|r| {
        *r.borrow_mut() = Some(buf);
    });
}

pub unsafe fn exception_jmp(status: std::ffi::c_int) -> ! {
    extern "C" {
        fn siglongjmp(buf: *const std::ffi::c_int, val: std::ffi::c_int) -> !;
    }

    JMP_BUF.with(|r| unsafe {
        if let Some(buf) = &*r.borrow() {
            siglongjmp(buf.as_ptr(), status);
        } else {
            std::process::exit(status);
        }
    })
}
