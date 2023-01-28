use crate::signal::{disable_raw_mode, post_print_fatal, pre_print_fatal, DlBacktrace};
use std::cell::RefCell;

pub fn init_crash_handler() {
    extern "C" {
        fn SetUnhandledExceptionFilter(
            filter: LpTopLevelExceptionFilter,
        ) -> LpTopLevelExceptionFilter;
    }

    unsafe {
        SetUnhandledExceptionFilter(handle);
    }
}

macro_rules! exception_code {
    ($($code:expr => $name:ident),* $(,)?) => {
        $(const $name: DWORD = $code;)*

        const fn code_name(code: DWORD) -> &'static str {
            match code {
                $($code => stringify!($name),)*
                _ => "unknown"
            }
        }
    };
}

exception_code! {
    0xC0000005 => STATUS_ACCESS_VIOLATION
}

unsafe extern "stdcall" fn handle(pinfo: *const ExceptionPointers) -> DWORD {
    let record = &*(*pinfo).exception_record;
    let code = record.exception_code;
    fn dl_get_name(addr: *const std::ffi::c_void) -> String {
        const MAX_PATH: usize = 260;

        extern "C" {
            fn GetModuleHandleExW(
                dw_flags: DWORD,
                addr: *const std::ffi::c_void,
                ph_module: *mut HMODULE,
            ) -> BOOL;

            fn GetModuleFileNameW(
                h_module: HMODULE,
                lp_filename: *mut WCHAR,
                n_size: DWORD,
            ) -> DWORD;
        }

        let mut module: HMODULE = 0;
        let mut buffer = [0 as WCHAR; MAX_PATH];
        let size;
        unsafe {
            if GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT
                    | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                addr,
                &mut module,
            ) == 0
            {
                return String::new();
            }

            size = GetModuleFileNameW(module, buffer.as_mut_ptr(), MAX_PATH as DWORD);
        }

        let mut slice = &buffer[..size as usize];

        if buffer.starts_with(&[92, 92, 63, 92]) {
            slice = &slice[4..];
        }

        String::from_utf16_lossy(slice)
    }

    let enabled = pre_print_fatal();
    crate::signal::fatal_error_print();

    eprintln!("exception address: {:p}", record.exception_address);
    eprintln!(
        "stack backtrace:\n{}",
        DlBacktrace {
            inner: backtrace::Backtrace::new(),
            fun: dl_get_name
        }
    );

    eprintln!(
        "Something went wrong, code: 0x{code:x}({})",
        code_name(code)
    );
    post_print_fatal(enabled);

    match code {
        STATUS_ACCESS_VIOLATION if crate::service::plugin::is_rec_enabled() => exception_jmp(code),
        _ => {
            disable_raw_mode();
            1
        }
    }
}

#[repr(C)]
struct ExceptionPointers {
    exception_record: *const ExceptionRecord,
    context_record: *const std::ffi::c_void,
}

#[repr(C)]
struct ExceptionRecord {
    exception_code: DWORD,
    exception_flags: DWORD,
    exception_record: *const ExceptionRecord,
    exception_address: *const std::ffi::c_void,
    number_parameters: DWORD,
    exception_information: [ULONG_PTR; EXCEPTION_MAXIMUM_PARAMETERS],
}

const EXCEPTION_MAXIMUM_PARAMETERS: usize = 15;

type LpTopLevelExceptionFilter = unsafe extern "stdcall" fn(*const ExceptionPointers) -> DWORD;

type DWORD = std::ffi::c_ulong;
type BOOL = std::ffi::c_int;
type HANDLE = usize;
type HMODULE = HANDLE;
type WCHAR = u16;

#[allow(non_camel_case_types)]
type ULONG_PTR = std::ffi::c_ulong;

const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: DWORD = 0x00000002;
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: DWORD = 0x00000004;

type _JBTYPE = [u64; 2];
const _JBLEN: usize = 16;
#[allow(non_camel_case_types)]
type jmp_buf = [_JBTYPE; _JBLEN];

thread_local! {
    static JMP_BUF: RefCell<Option<jmp_buf>> = RefCell::new(None);
}

pub unsafe fn save_jmp() {
    extern "C" {
        fn setjmp(buf: *mut _JBTYPE) -> std::ffi::c_int;
    }

    let mut buf: jmp_buf = [[0, 0]; _JBLEN];

    /*
    let ret = unsafe { setjmp(buf.as_mut_ptr()) };

    if ret != 0 {
        panic!("exception occurred, status: {ret}");
    }
    */

    JMP_BUF.with(|r| {
        *r.borrow_mut() = Some(buf);
    });
}

pub unsafe fn exception_jmp(status: DWORD) -> ! {
    extern "C" {
        fn longjmp(buf: *const _JBTYPE, val: std::ffi::c_int) -> !;
    }

    JMP_BUF.with(|r| unsafe {
        if let Some(buf) = &*r.borrow() {
            longjmp(buf.as_ptr(), status as _);
        } else {
            std::process::exit(status as i32);
        }
    })
}
