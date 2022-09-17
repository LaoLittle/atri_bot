use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{fs, io};

use crate::terminal::{INPUT_BUFFER, PROMPT};
use tracing::{error, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::OffsetTime;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logger() -> (WorkerGuard, WorkerGuard) {
    let time_format =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap();

    let (s, s_guard) = tracing_appender::non_blocking(LogStdoutWriter);

    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(s.with_max_level(Level::DEBUG));

    let (f, f_guard) = tracing_appender::non_blocking(LogFileWriter::default());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(f.with_max_level(Level::DEBUG));

    let (stdout_layer, file_layer, err) = match time::UtcOffset::current_local_offset() {
        Ok(ofs) => {
            let time = OffsetTime::new(ofs, time_format);
            (
                stdout_layer.with_timer(time.clone()),
                file_layer.with_timer(time),
                None,
            )
        }
        Err(e) => {
            let time = OffsetTime::new(time::UtcOffset::from_hms(8, 0, 0).unwrap(), time_format);

            (
                stdout_layer.with_timer(time.clone()),
                file_layer.with_timer(time),
                Some(e),
            )
        }
    };

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .init();

    if let Some(e) = err {
        error!("{}", e);
    }

    (s_guard, f_guard)
}

pub struct LogStdoutWriter;

impl Default for LogStdoutWriter {
    fn default() -> Self {
        Self
    }
}

impl Write for LogStdoutWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut stdout = io::stdout().lock();

        stdout.write_all(&[13])?;
        let size = stdout.write(buf)?;

        stdout.write_all(PROMPT)?;
        stdout.write_all(INPUT_BUFFER.read().unwrap().as_bytes())?;
        stdout.flush()?;

        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}

pub struct LogFileWriter {
    output: &'static Path,
}

/*fn ansi_filter() -> &'static Regex {
    use regex::Regex;
    static ANSI_FILTER: OnceLock<Regex> = OnceLock::new();

    ANSI_FILTER.get_or_init(|| {
        Regex::new("\\x1b\\[([0-9,A-Z]{1,2}(;[0-9]{1,2})?(;[0-9]{3})?)?[m|K]?").unwrap()
    })
}*/

impl Default for LogFileWriter {
    fn default() -> Self {
        let path = get_latest_log_file();

        Self { output: path }
    }
}

static LOG_FILE_OPENED: OnceLock<File> = OnceLock::new();

impl Write for LogFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let f = LOG_FILE_OPENED.get_or_try_init(|| File::create(self.output));

        f.and_then(|mut f| f.write(buf))
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
    LOG_FILE
        .get_or_init(|| {
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
        })
        .as_path()
}
