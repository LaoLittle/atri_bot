use std::ffi::c_int;
use std::io::Write;
use std::sync::RwLock;
use crossterm::cursor::MoveToColumn;
use crossterm::execute;
use crossterm::style::Print;

pub static OUTPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

pub static INPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

const BUFFER_SIZE: usize = 4096;

pub const PROMPT: &[u8] = b">> ";

struct RawStdout {
    fd: c_int,
}

impl RawStdout {
    fn next_line(&mut self) -> Result<(), std::io::Error> {
        execute!(self, Print('\n'), MoveToColumn(0))
    }
}

impl Write for RawStdout {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = unsafe { libc::write(self.fd, buf.as_ptr() as _, buf.len() as _) };

        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(n as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const STDOUT_FILENO: c_int = 1;

pub fn handle_standard_output() -> std::io::Result<()> {
    let mut pipe = [0; 2];

    let stdout_bak = unsafe { libc::dup(STDOUT_FILENO) };

    let mut buf = [b'\0'; BUFFER_SIZE];
    unsafe {
        libc::pipe(pipe.as_mut_ptr());

        let stat = libc::dup2(pipe[1], STDOUT_FILENO);

        if stat == -1 {
            return Err(std::io::Error::last_os_error());
        }

        let mut stdout_fd = RawStdout { fd: stdout_bak };

        loop {
            let size = libc::read(pipe[0], buf.as_mut_ptr() as _, BUFFER_SIZE as _);

            if size == -1 {
                eprintln!("Error: {}", std::io::Error::last_os_error());
                break;
            }

            if size == 1 && buf[0] == b'\n' {
                stdout_fd.next_line()?;
                continue;
            }

            let split: Vec<&[u8]> = buf[..size as usize].split(|&b| b == b'\n').collect();
            let mut split = split.into_iter();

            if split.len() == 1 {
                let slice = split.next().unwrap();

                if slice.is_empty() {
                    stdout_fd.next_line()?;
                }

                stdout_fd.write_all(slice)?;
                continue;
            }

            let last = split.len().saturating_sub(1);
            for (i, slice) in split.enumerate() {
                if i == last {
                    if !slice.is_empty() {
                        stdout_fd.write_all(slice)?;
                    }

                    continue;
                }

                if slice.is_empty() {
                    stdout_fd.next_line()?;
                    continue;
                }

                stdout_fd.write_all(slice)?;
                stdout_fd.next_line()?;
            }
        }

        libc::dup2(stdout_bak, STDOUT_FILENO);
    }

    Ok(())
}