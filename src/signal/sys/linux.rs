use crate::signal::{disable_raw_mode, post_print_fatal, pre_print_fatal, DlBacktrace};
use std::mem::MaybeUninit;
use std::ptr::null_mut;

pub fn init_crash_handler() {
    let mut act = Sigaction {
        __sigaction_u: SigactionU {
            __sa_sigaction: Some(handle),
        },
        sa_mask: SigsetT::default(),
        sa_flags: 0,
        sa_restorer: None,
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
            if sigaction(sig, &act, null_mut()) != 0 {
                eprintln!("signal {} 注册失败", sig);
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
                _ => "UNKNOWN"
            }
        }
    };
}

exception_sig! {
    6 => SIGABRT,
    7 => SIGBUS,
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

    eprintln!("addr: {:p}", (*_info)._sifields._sigfault.si_addr);
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
        }
    }
}

type PidT = std::ffi::c_int;
type UidT = std::ffi::c_uint;

const _SIGSET_NWORDS: usize = 1024 / (8 * std::mem::size_of::<std::ffi::c_ulong>());

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
struct SigsetT {
    __val: [std::ffi::c_ulong; _SIGSET_NWORDS],
}

//pub type SigsetT = DarwinSigsetT;

#[repr(C)]
#[derive(Copy, Clone)]
struct Siginfo {
    pub si_signo: std::ffi::c_int,
    pub si_errno: std::ffi::c_int,
    pub si_code: std::ffi::c_int,
    #[cfg(target_pointer_width = "64")]
    __pad0: std::ffi::c_int,
    pub _sifields: Sifields,
}

const __SI_MAX_SIZE: usize = 128;
const __SI_PAD_SIZE: usize = (__SI_MAX_SIZE / std::mem::size_of::<std::ffi::c_int>()) - 4;

#[repr(C)]
#[derive(Copy, Clone)]
union Sifields {
    _pad: [std::ffi::c_int; __SI_PAD_SIZE],
    _kill: SiKill,
    _timer: SiTimer,
    _rt: SiRt,
    _sigchld: Sigchld,
    _sigfault: Sigfault,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct SiKill {
    si_pid: PidT,
    si_uid: UidT,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct SiTimer {
    si_tid: std::ffi::c_int,
    si_overrun: std::ffi::c_int,
    si_sigval: Sigval,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct SiRt {
    si_pid: PidT,
    si_uid: UidT,
    si_sigval: Sigval,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Sigchld {
    si_pid: PidT,
    si_uid: UidT,
    si_status: std::ffi::c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Sigfault {
    si_addr: *mut std::ffi::c_void,
    si_addr_lsb: std::ffi::c_short,
    _bounds: Bounds,
}

#[repr(C)]
#[derive(Copy, Clone)]
union Bounds {
    _addr_bnd: AddrBnd,
    _pkey: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct AddrBnd {
    _lower: *mut std::ffi::c_void,
    _upper: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Sigval {
    pub sival_int: std::ffi::c_int,
    pub sival_ptr: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
union SigactionU {
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
struct Sigaction {
    pub __sigaction_u: SigactionU,
    pub sa_mask: SigsetT,
    pub sa_flags: std::ffi::c_int,
    pub sa_restorer: Option<unsafe extern "C" fn()>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct DlInfo {
    pub dli_fname: *const std::ffi::c_char,
    pub dli_fbase: *mut std::ffi::c_void,
    pub dli_sname: *const std::ffi::c_char,
    pub dli_saddr: *mut std::ffi::c_void,
}

pub unsafe fn save_jmp() {
    // todo: sigsetjmp
}

pub fn exception_jmp(status: std::ffi::c_int) -> ! {
    std::process::exit(status); // todo: siglongjmp
}
