use crate::signal::DlBacktrace;
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
            if sigaction(sig, &act, null_mut()) != 0 {
                eprintln!("signal {} 注册失败", sig);
            }
        }
    }
}

pub const SIGABRT: std::ffi::c_int = 6;
pub const SIGBUS: std::ffi::c_int = 10;
pub const SIGSEGV: std::ffi::c_int = 11;

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

    crate::signal::fatal_error_print();

    let bt = backtrace::Backtrace::new();

    eprintln!("addr: {:p}", (*_info).si_addr);
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

pub type DarwinPidT = i32;
pub type DarwinUidT = u32;

pub type DarwinSigsetT = u32;

pub type PidT = DarwinPidT;
pub type UidT = DarwinUidT;

pub type SigsetT = DarwinSigsetT;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Siginfo {
    pub si_signo: std::os::raw::c_int,
    pub si_errno: std::os::raw::c_int,
    pub si_code: std::os::raw::c_int,
    pub si_pid: PidT,
    pub si_uid: UidT,
    pub si_status: std::os::raw::c_int,
    pub si_addr: *mut std::os::raw::c_void,
    pub si_value: Sigval,
    pub si_band: std::os::raw::c_long,
    pub __pad: [std::os::raw::c_ulong; 7usize],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Sigval {
    pub sival_int: std::os::raw::c_int,
    pub sival_ptr: *mut std::os::raw::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union SigactionU {
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
pub struct Sigaction {
    pub __sigaction_u: SigactionU,
    pub sa_mask: SigsetT,
    pub sa_flags: std::os::raw::c_int,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DlInfo {
    pub dli_fname: *const std::os::raw::c_char,
    pub dli_fbase: *mut std::os::raw::c_void,
    pub dli_sname: *const std::os::raw::c_char,
    pub dli_saddr: *mut std::os::raw::c_void,
}
