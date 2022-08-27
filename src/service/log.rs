use regex::Regex;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{fs, io};

use crate::terminal::{INPUT_BUFFER, PROMPT};
use tracing::{error, Level};
use tracing_subscriber::fmt::time::{OffsetTime, UtcTime};
use tracing_subscriber::FmtSubscriber;

pub fn init_logger() {
    let time_format =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")
            .expect("Unknown time formatting");

    let builder = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(false)
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

fn ansi_filter() -> &'static Regex {
    static ANSI_FILTER: OnceLock<Regex> = OnceLock::new();

    ANSI_FILTER.get_or_init(|| {
        Regex::new("\\x1b\\[([0-9,A-Z]{1,2}(;[0-9]{1,2})?(;[0-9]{3})?)?[m|K]?").unwrap()
    })
}

impl LogWriter {}

impl Default for LogWriter {
    fn default() -> Self {
        let path = get_latest_log_file();

        Self { output: path }
    }
}

static LOG_FILE_OPENED: OnceLock<File> = OnceLock::new();

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let size: usize;
        {
            let mut stdout = io::stdout().lock();

            stdout.write_all(&[13])?;
            size = stdout.write(buf)?;

            stdout.write_all(PROMPT)?;
            stdout.write_all(INPUT_BUFFER.read().unwrap().as_bytes())?;
            stdout.flush()?;
        }

        let f = LOG_FILE_OPENED.get_or_try_init(|| File::create(self.output));

        if let Err(e) = f.and_then(|mut f| {
            let str = String::from_utf8_lossy(buf);
            let no_ansi = ansi_filter().replace_all(&str, "");
            f.write_all(no_ansi.as_bytes())?;
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
