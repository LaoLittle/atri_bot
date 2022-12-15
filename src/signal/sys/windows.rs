use crate::signal::DlBacktrace;

pub fn init_signal_hook() {
    extern "C" {
        fn SetUnhandledExceptionFilter(
            filter: LpTopLevelExceptionFilter,
        ) -> LpTopLevelExceptionFilter;
    }

    unsafe {
        SetUnhandledExceptionFilter(handle);
    }
}

unsafe extern "C" fn handle(_: *const std::os::raw::c_void) -> DWORD {
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
                lp_filename: *const WCHAR,
                n_size: DWORD,
            ) -> DWORD;
        }

        let mut module = 0 as HMODULE;
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

        let slice = if buffer.starts_with(&[92, 92, 63, 92]) {
            &buffer[4..size as usize]
        } else {
            &buffer[..size as usize]
        };

        String::from_utf16_lossy(slice)
    }

    let bt = backtrace::Backtrace::new();
    eprintln!(
        "stack backtrace:\n{}",
        DlBacktrace {
            inner: bt,
            fun: dl_get_name
        }
    );

    eprintln!("Something went wrong.");

    1
}

type LpTopLevelExceptionFilter = unsafe extern "C" fn(*const std::os::raw::c_void) -> DWORD;

type DWORD = std::os::raw::c_ulong;

type BOOL = std::os::raw::c_int;

const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: DWORD = 0x00000002;
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: DWORD = 0x00000004;

type HANDLE = usize;
type HMODULE = HANDLE;

type WCHAR = u16;
