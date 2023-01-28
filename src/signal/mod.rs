use std::collections::HashSet;
use std::fmt;
use std::fmt::{Formatter, Write};
use std::path::Path;

mod sys;
pub use sys::init_crash_handler;
pub(crate) use sys::save_jmp;

struct DlBacktrace {
    pub inner: backtrace::Backtrace,
    pub fun: fn(*const std::ffi::c_void) -> String,
}

impl fmt::Display for DlBacktrace {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut frame_back = HashSet::new();
        for (frame_cnt, frame) in self.inner.frames().iter().enumerate() {
            let addr = frame.symbol_address();
            let fname = (self.fun)(addr);

            write!(f, "{frame_cnt} File ")?;
            f.write_str(&fname)?;
            f.write_str(": \n")?;

            frame_back.insert(fname);

            let symbols = frame.symbols();

            if symbols.len() == 0 {
                writeln!(f, "  at {:p}", addr)?;
            }

            for symbol in symbols {
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

fn pre_print_fatal() -> bool {
    let enabled = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    let _ = crossterm::terminal::disable_raw_mode();
    enabled
}

fn post_print_fatal(enabled: bool) {
    if enabled {
        disable_raw_mode();
    }
}

fn disable_raw_mode() {
    let _ = crossterm::terminal::disable_raw_mode();
}
