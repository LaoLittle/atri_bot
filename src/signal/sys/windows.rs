use crate::signal::{disable_raw_mode, post_print_fatal, pre_print_fatal, DlBacktrace};

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

unsafe extern "stdcall" fn handle(_: *const ExceptionPointers) -> DWORD {
    fn dl_get_name(addr: *const std::ffi::c_void) -> String {
        const MAX_PATH: usize = 260;

        extern "C" {
            fn GetModuleHandleExW(
                dw_flags: DWORD,
                addr: *const std::os::raw::c_void,
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
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT
                    | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                addr as _,
                &mut module,
            );

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

    let bt = backtrace::Backtrace::new();
    eprintln!(
        "stack backtrace:\n{}",
        DlBacktrace {
            inner: bt,
            fun: dl_get_name
        }
    );

    eprintln!("Something went wrong.");
    post_print_fatal(enabled);

    {
        disable_raw_mode();
    }

    1
}

type ExceptionPointers = std::os::raw::c_void; // FIXME

type LpTopLevelExceptionFilter = unsafe extern "stdcall" fn(*const ExceptionPointers) -> DWORD;

type DWORD = std::os::raw::c_ulong;

type BOOL = std::os::raw::c_int;

const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: DWORD = 0x00000002;
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: DWORD = 0x00000004;

type HANDLE = usize;
type HMODULE = HANDLE;

type WCHAR = u16;

pub fn save_jmp() -> std::ffi::c_int {
    0 // todo: RtlCaptureContext
}

pub fn expection_jmp(status: std::ffi::c_int) -> ! {
    std::process::exit(status); // todo: RtlRestoreContext
}
