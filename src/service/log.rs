use crate::terminal::{INPUT_BUFFER, PROMPT, TERMINAL_CLOSED};
use std::io;
use std::io::Write;
use std::sync::atomic::Ordering;
use tracing::{warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::OffsetTime;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logger() -> [WorkerGuard; 3] {
    let local_offset = time::UtcOffset::current_local_offset();

    let time_format =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap();

    let (s, s_guard) = tracing_appender::non_blocking(LogStdoutWriter);

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(s.with_max_level(Level::DEBUG));

    let file_writer = tracing_appender::rolling::daily("log", "atri_bot.log");
    let (f, f_guard) = tracing_appender::non_blocking(file_writer);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(f.with_max_level(Level::INFO));

    let file_error_writer = tracing_appender::rolling::daily("log/error", "atri_bot.err");
    let (f_err, f_err_guard) = tracing_appender::non_blocking(file_error_writer);

    let file_error_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(f_err.with_max_level(Level::ERROR));

    let (offset, err) = match local_offset {
        Ok(ofs) => (ofs, None),
        Err(e) => (time::UtcOffset::from_hms(8, 0, 0).unwrap(), Some(e)),
    };

    let timer = OffsetTime::new(offset, time_format);
    let (stdout_layer, file_layer, file_error_layer) = (
        stdout_layer.with_timer(timer.clone()),
        file_layer.with_timer(timer.clone()),
        file_error_layer.with_timer(timer),
    );

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .with(file_error_layer)
        .init();

    if let Some(e) = err {
        warn!("初始化日志时间错误: {}, 使用默认时区UTC+8", e);
    }

    [s_guard, f_guard, f_err_guard]
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

        if TERMINAL_CLOSED.load(Ordering::Relaxed) {
            return Ok(buf.len());
        }

        stdout.write_all(&[13])?;
        let size = stdout.write(buf)?;

        stdout.write_all(PROMPT)?;

        if let Ok(rw) = INPUT_BUFFER.try_read() {
            stdout.write_all(rw.as_bytes())?;
        }

        stdout.flush()?;

        Ok(size)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}
