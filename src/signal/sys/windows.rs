use crate::signal::{disable_raw_mode, post_print_fatal, pre_print_fatal, DlBacktrace};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use winapi::um::winnt::{RtlCaptureContext, RtlRestoreContext, CONTEXT};

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
                return String::from("unknown");
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

thread_local! {
    static CONTEXT: RefCell<Option<CONTEXT>> = RefCell::new(None);
    static STATUS: AtomicU32 = AtomicU32::new(0);
}

pub unsafe fn save_jmp() {
    let mut buf = std::mem::zeroed::<CONTEXT>();

    unsafe {
        RtlCaptureContext(&mut buf);
    }

    let status = STATUS.with(|r| r.load(Ordering::Relaxed));
    if status != 0 {
        panic!("exception occurred, status: 0x{status:x}");
    }

    CONTEXT.with(|r| {
        *r.borrow_mut() = Some(buf);
    });
}

pub unsafe fn exception_jmp(status: u32) -> ! {
    CONTEXT.with(|r| unsafe {
        let mut brr = r.borrow_mut();
        if let Some(ctx) = &mut *brr {
            STATUS.with(|r| r.store(status, Ordering::Relaxed));
            RtlRestoreContext(ctx, std::ptr::null_mut());
        }

        std::process::exit(status as i32);
    })
}
