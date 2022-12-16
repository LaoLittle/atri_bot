use crate::signal::DlBacktrace;
use std::mem::MaybeUninit;
use std::ptr::null_mut;

pub fn init_signal_hook() {
    let mut act = Sigaction {
        __sigaction_u: SigactionU {
            __sa_sigaction: Some(handle),
        },
        sa_mask: SigsetT::default(),
        sa_flags: 0,
        sa_restorer: None,
    };

    extern "C" {
        fn sigemptyset(arg1: *mut SigsetT) -> std::os::raw::c_int;

        fn sigaction(
            arg1: std::os::raw::c_int,
            arg2: *const Sigaction,
            arg3: *mut Sigaction,
        ) -> std::os::raw::c_int;
    }

    unsafe {
        sigemptyset(&mut act.sa_mask);

        for sig in [SIGABRT, SIGSEGV, SIGBUS] {
            sigaction(sig, &act, null_mut());
        }
    }
}

const SIGABRT: std::ffi::c_int = 6;
const SIGBUS: std::ffi::c_int = 10;
const SIGSEGV: std::ffi::c_int = 11;

unsafe extern "C" fn handle(
    sig: std::os::raw::c_int,
    _info: *mut Siginfo,
    _addr: *mut std::os::raw::c_void,
) {
    fn dl_get_name(addr: *const std::ffi::c_void) -> String {
        extern "C" {
            fn dladdr(arg1: *const std::os::raw::c_void, arg2: *mut DlInfo) -> std::os::raw::c_int;
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

    let bt = backtrace::Backtrace::new();

    eprintln!("addr: {:p}", (*_info)._sifields._sigfault.si_addr);
    eprintln!(
        "stack backtrace:\n{}",
        DlBacktrace {
            inner: bt,
            fun: dl_get_name
        }
    );

    eprintln!("Something went wrong, signal: {}", sig);

    std::process::exit(sig as i32);
}

type DarwinSigsetT = u32;

type PidT = std::os::raw::c_int;
type UidT = std::os::raw::c_uint;

const _SIGSET_NWORDS: usize = 1024 / (8 * std::mem::size_of::<std::os::raw::c_ulong>());

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
struct SigsetT {
    __val: [std::os::raw::c_ulong; _SIGSET_NWORDS],
}

//pub type SigsetT = DarwinSigsetT;

#[repr(C)]
#[derive(Copy, Clone)]
struct Siginfo {
    pub si_signo: std::os::raw::c_int,
    pub si_errno: std::os::raw::c_int,
    pub si_code: std::os::raw::c_int,
    #[cfg(target_pointer_width = "64")]
    __pad0: std::os::raw::c_int,
    pub _sifields: Sifields,
}

const __SI_MAX_SIZE: usize = 128;
const __SI_PAD_SIZE: usize = (__SI_MAX_SIZE / std::mem::size_of::<std::os::raw::c_int>()) - 4;

#[repr(C)]
#[derive(Copy, Clone)]
union Sifields {
    _pad: [std::os::raw::c_int; __SI_PAD_SIZE],
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
    si_tid: std::os::raw::c_int,
    si_overrun: std::os::raw::c_int,
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
    si_status: std::os::raw::c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Sigfault {
    si_addr: *mut std::os::raw::c_void,
    si_addr_lsb: std::os::raw::c_short,
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
    _lower: *mut std::os::raw::c_void,
    _upper: *mut std::os::raw::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Sigval {
    pub sival_int: std::os::raw::c_int,
    pub sival_ptr: *mut std::os::raw::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
union SigactionU {
    pub __sa_handler: Option<unsafe extern "C" fn(arg1: std::os::raw::c_int)>,
    pub __sa_sigaction: Option<
        unsafe extern "C" fn(
            arg1: std::os::raw::c_int,
            arg2: *mut Siginfo,
            arg3: *mut std::os::raw::c_void,
        ),
    >,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Sigaction {
    pub __sigaction_u: SigactionU,
    pub sa_mask: SigsetT,
    pub sa_flags: std::os::raw::c_int,
    pub sa_restorer: Option<unsafe extern "C" fn()>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct DlInfo {
    pub dli_fname: *const std::os::raw::c_char,
    pub dli_fbase: *mut std::os::raw::c_void,
    pub dli_sname: *const std::os::raw::c_char,
    pub dli_saddr: *mut std::os::raw::c_void,
}
