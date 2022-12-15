use std::fmt;
use std::fmt::Formatter;
use std::path::Path;

mod sys;
pub use sys::init_signal_hook;

struct DlBacktrace {
    pub inner: backtrace::Backtrace,
    pub fun: fn(*const std::ffi::c_void) -> *const std::ffi::c_char,
}

impl fmt::Display for DlBacktrace {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut frame_cnt = 0;
        for frame in self.inner.frames() {
            let fname = (self.fun)(frame.symbol_address());

            write!(f, "{frame_cnt} File: {:?}: \n", unsafe {
                std::ffi::CStr::from_ptr(fname)
            })?;

            for symbol in frame.symbols() {
                print!(
                    "    {}\n at {}",
                    symbol.name().unwrap_or(backtrace::SymbolName::new(&[])),
                    symbol.filename().and_then(Path::to_str).unwrap_or(""),
                );

                match (symbol.lineno(), symbol.colno()) {
                    (Some(line), Some(column)) => print!(":{line}:{column}"),
                    _ => {}
                }

                println!();
            }

            frame_cnt += 1;
        }

        Ok(())
    }
}
