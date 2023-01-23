use std::collections::HashSet;
use std::fmt;
use std::fmt::{Formatter, Write};
use std::path::Path;

mod sys;
pub use sys::init_crash_handler;

struct DlBacktrace {
    pub inner: backtrace::Backtrace,
    pub fun: fn(*const std::ffi::c_void) -> String,
}

impl fmt::Display for DlBacktrace {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut frame_back = HashSet::new();
        for (frame_cnt, frame) in self.inner.frames().iter().enumerate() {
            let fname = (self.fun)(frame.symbol_address());

            write!(f, "{frame_cnt} File ")?;
            f.write_str(&fname)?;
            f.write_str(": \n")?;

            frame_back.insert(fname);

            for symbol in frame.symbols() {
                writeln!(
                    f,
                    "    {}",
                    symbol.name().unwrap_or(backtrace::SymbolName::new(&[])),
                )?;

                if let Some(filename) = symbol.filename().and_then(Path::to_str) {
                    write!(f, "  at {}", filename)?;
                }

                match (symbol.lineno(), symbol.colno()) {
                    (Some(line), Some(column)) => write!(f, ":{line}:{column}")?,
                    (Some(line), None) => write!(f, ":{line}")?,
                    (None, Some(column)) => write!(f, ":?:{column}")?,
                    _ => {}
                }

                writeln!(f)?;
            }
        }

        f.write_str("--------Frames--------\n")?;
        for frame in frame_back {
            f.write_str(&frame)?;
            f.write_char('\n')?;
        }
        f.write_str("----------------------\n")?;

        writeln!(f, "\ncurrent thread: {:?}", std::thread::current())?;

        Ok(())
    }
}

fn fatal_error_print() {
    eprintln!("An fatal error has been detected.");
}
