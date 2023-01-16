use crate::config::log::LogConfig;
use crate::config::service::ServiceConfig;
use crate::terminal::buffer::{INPUT_BUFFER, TERMINAL_CLOSED};
use crate::terminal::PROMPT;
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
    let config = ServiceConfig::<LogConfig>::new("log", crate::config::log::DEFAULT_CONFIG).read();

    let local_offset = time::UtcOffset::current_local_offset();

    let mut errors = Vec::with_capacity(2);
    let time_format = time::format_description::parse(config.time_format.leak())
        .or_else(|e| {
            errors.push(format!("日志时间格式错误: {e}, 将使用默认时间格式"));
            time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")
        })
        .unwrap();

    let (s, s_guard) = tracing_appender::non_blocking(LogStdoutWriter);

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(s.with_max_level(config.max_level.as_tracing_level()));

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

    let offset = match local_offset {
        Ok(ofs) => ofs,
        Err(e) => {
            errors.push(format!("初始化日志时间错误: {e}, 将使用默认时区UTC+8"));
            time::UtcOffset::from_hms(8, 0, 0).unwrap()
        }
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

    for error in errors {
        warn!("{error}");
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
        fn write_content<W: Write>(mut out: W, buf: &[u8]) -> io::Result<usize> {
            if TERMINAL_CLOSED.load(Ordering::Relaxed) {
                return Ok(buf.len());
            }

            out.write_all(&[13])?;
            let size = out.write(buf)?;
            out.write_all(PROMPT)?;

            if let Ok(rw) = INPUT_BUFFER.try_read() {
                out.write_all(rw.as_bytes())?;
            }

            out.flush()?;

            Ok(size)
        }

        write_content(io::stdout().lock(), buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}
