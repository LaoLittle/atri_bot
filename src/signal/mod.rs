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

            writeln!(f, "{frame_cnt} File {fname}: ")?;

            frame_back.insert(fname);

            for symbol in frame.symbols() {
                write!(
                    f,
                    "    {}\n at {}",
                    symbol.name().unwrap_or(backtrace::SymbolName::new(&[])),
                    symbol
                        .filename()
                        .and_then(Path::to_str)
                        .unwrap_or("unknown"),
                )?;

                match (symbol.lineno(), symbol.colno()) {
                    (Some(line), Some(column)) => write!(f, ":{line}:{column}")?,
                    (Some(line), None) => write!(f, ":{line}")?,
                    _ => {}
                }

                writeln!(f)?;
            }
        }

        f.write_str("\n--------Frames--------\n")?;
        for frame in frame_back {
            f.write_str(&frame)?;
            f.write_char('\n')?;
        }
        f.write_str("----------------------\n")?;

        Ok(())
    }
}

fn fatal_error_print() {
    eprintln!("An fatal error has been detected");
}
