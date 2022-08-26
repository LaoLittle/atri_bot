use std::error::Error;
use std::io::{stdout, Write};
use std::sync::RwLock;

use crossterm::cursor::MoveToNextLine;
use crossterm::event::KeyCode;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{event, execute};
use tracing::error;

pub static OUTPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

pub static INPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

const NEXT_LINE: &[u8] = concat!("\x1B[", "1E").as_bytes();
pub fn handle_standard_output() -> bool {
    const BUFFER_SIZE: usize = 4096;
    let mut pipe = [0; 2];

    let stdout_bak = unsafe { libc::dup(libc::STDOUT_FILENO) };

    let mut buf = [b'\0'; BUFFER_SIZE];

    let next_line = if crossterm::terminal::is_raw_mode_enabled().unwrap_or(false) {
        NEXT_LINE
    } else {
        b"\n"
    };
    unsafe {
        libc::pipe(pipe.as_mut_ptr());
        let stat = libc::dup2(pipe[1], libc::STDOUT_FILENO);

        if stat == -1 {
            return false;
        }

        loop {
            let size = libc::read(pipe[0], buf.as_mut_ptr() as _, BUFFER_SIZE);

            if size == -1 {
                error!("Error: {}", std::io::Error::last_os_error());
                break;
            }

            let mut split = buf[..size as usize].split(|&b| b == b'\n');

            if let Some(s) = split.next() {
                if s.is_empty() {
                    libc::write(stdout_bak, next_line.as_ptr() as _, next_line.len());
                }

                libc::write(stdout_bak, s.as_ptr() as _, s.len());
            }

            for slice in split {
                let slice: &[u8] = slice;
                if slice.is_empty() {
                    libc::write(stdout_bak, next_line.as_ptr() as _, next_line.len());

                    continue;
                }

                libc::write(stdout_bak, next_line.as_ptr() as _, next_line.len());
                libc::write(stdout_bak, slice.as_ptr() as _, slice.len());
            }
        }

        libc::dup2(stdout_bak, libc::STDOUT_FILENO);
    }

    true
}

pub fn start_read_input() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;

    while let Ok(e) = event::read() {
        match e {
            event::Event::Key(k) => {
                match k.code {
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
                        let rl = INPUT_BUFFER.read()?;

                        let mut stdout = stdout().lock();
                        execute!(stdout, MoveToNextLine(1))?;

                        let cmd = rl.trim_end();
                        match cmd {
                            "" => {
                                // nothing to do
                            }
                            "help" | "?" => {
                                static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                                for &info in INFOS {
                                    stdout.write_all(info.as_bytes())?;
                                    stdout.write_all(b"\n")?;
                                }
                            }
                            "exit" | "quit" | "stop" => {
                                println!("正在停止AtriQQ");
                                break;
                            }
                            _ => {
                                println!("未知的命令 '{}', 使用 'help' 显示帮助信息", cmd);
                            }
                        }

                        drop(rl);
                        INPUT_BUFFER.write()?.clear();
                        stdout.write_all(b">>")?;
                        stdout.flush()?;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

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
