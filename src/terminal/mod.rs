use std::error::Error;
use std::ffi::c_int;
use std::io::{stdout, Write};
use std::mem;
use std::ops::DerefMut;
use std::sync::RwLock;

use crate::service::command::{handle_plugin_command, PLUGIN_COMMAND};
use crate::PluginManager;
use crossterm::cursor::MoveToColumn;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, KeyCode};
use crossterm::style::Print;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{event, execute};
use event::Event;
use tracing::{error, info};

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

#[cfg(windows)]
pub fn handle_standard_output() -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
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

            let last = split.len().checked_sub(1).unwrap_or(0);
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

pub fn start_read_input(manager: &mut PluginManager) -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;

    let _ = execute!(stdout(), EnableBracketedPaste);

    while let Ok(e) = event::read() {
        match e {
            Event::Key(k) => match k.code {
                KeyCode::Char(c) => {
                    INPUT_BUFFER.write()?.push(c);
                    let mut stdout = stdout().lock();
                    stdout.write_all(&[c as u8])?;
                    stdout.flush()?;
                }
                KeyCode::Backspace => {
                    if let Some(_) = INPUT_BUFFER.write()?.pop() {
                        let mut stdout = stdout().lock();
                        stdout.write_all(&[8, b' ', 8])?;
                        stdout.flush()?;
                    };
                }
                KeyCode::Enter => {
                    let input = {
                        let mut wl = INPUT_BUFFER.write()?;
                        let s = mem::take(wl.deref_mut());
                        wl.clear();
                        s
                    };

                    let mut stdout = stdout().lock();
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;

                    let cmd = input.trim_end();
                    match cmd {
                        "" => {
                            stdout.write_all(PROMPT)?;
                            stdout.flush()?;
                        }
                        "help" | "?" => {
                            static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                            let mut s = String::from('\n');
                            for &info in INFOS {
                                s.push_str(info);
                                s.push('\n');
                            }
                            s.pop();
                            info!("{}", s);
                        }
                        "exit" | "quit" | "stop" => {
                            info!("正在停止AtriQQ");
                            break;
                        }
                        plugin if plugin.starts_with(PLUGIN_COMMAND) => {
                            if let Err(e) = handle_plugin_command(plugin, manager) {
                                error!("{}", e);
                            }
                        }
                        _ => {
                            info!("未知的命令 '{}', 使用 'help' 显示帮助信息", cmd);
                        }
                    }
                }
                _ => {}
            },
            Event::Paste(s) => {
                INPUT_BUFFER.write()?.push_str(&s);
                let mut stdout = stdout().lock();
                stdout.write_all(s.as_bytes())?;
                stdout.flush()?;
            }
            _ => {}
        }
    }

    let _ = execute!(stdout(), DisableBracketedPaste);

    disable_raw_mode()?;
    Ok(())
}

pub struct AtriTerminal {
    pub input_buffer: RwLock<String>,
}

impl AtriTerminal {
    pub fn new() -> Self {
        Self {
            input_buffer: RwLock::new(String::new()),
        }
    }
}
