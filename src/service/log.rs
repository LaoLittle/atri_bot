use std::{fs, io};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing::{error, Level};
use tracing_subscriber::fmt::time::{OffsetTime, UtcTime};
use tracing_subscriber::FmtSubscriber;

pub fn init_logger() {
    let time_format = time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").expect("Unknown time formatting");

    let builder = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_writer(LogWriter::default);

    if let Ok(ofs) = time::UtcOffset::current_local_offset() {
        builder.with_timer(OffsetTime::new(ofs, time_format)).init();
    } else {
        builder.with_timer(UtcTime::new(time_format)).init();
    };
}

pub struct LogWriter {
    output: &'static Path,
}

impl LogWriter {}

impl Default for LogWriter {
    fn default() -> Self {
        let path = get_latest_log_file();

        Self {
            output: path
        }
    }
}

static LOG_FILE_OPENED: OnceLock<File> = OnceLock::new();

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let size: usize;
        {
            let mut stdout = io::stdout().lock();
            stdout.write(&[13])?;
            size = stdout.write(buf)?;

            stdout.write(b">>")?;
            stdout.flush()?;
        }

        let f = LOG_FILE_OPENED.get_or_try_init(|| {
            File::create(self.output)
        });

        if let Err(e) = f.and_then(|mut f| {
            f.write(buf)?;
            Ok(())
        }) {
            error!("Log写入失败: {}", e);
        }

        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()?;
        Ok(())
    }
}

static LOG_PATH: &str = "log";

pub fn log_dir_buf() -> PathBuf {
    PathBuf::from(LOG_PATH)
}

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

fn get_latest_log_file() -> &'static Path {
    LOG_FILE.get_or_init(|| {
        let mut buf = log_dir_buf();
        let _ = fs::create_dir_all(&buf);
        buf.push("latest.log");

        if buf.is_file() {
            let mut dir = log_dir_buf();

            let mut i = 1;

            fn push_buf(buf: &mut PathBuf, i: u32) {
                buf.push(format!("log-{}.log", i));
            }

            push_buf(&mut dir, i);

            while dir.is_file() {
                i += 1;
                dir.pop();
                push_buf(&mut dir, i);
            }

            let _ = fs::copy(&buf, &dir);
        }

        buf
    }).as_path()
}