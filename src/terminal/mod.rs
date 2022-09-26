use cfg_if::cfg_if;
use std::error::Error;
use std::io::{stdout, Write};
use std::sync::RwLock;

use crate::service::command::{handle_plugin_command, PLUGIN_COMMAND};
use crate::PluginManager;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{event, execute};
use event::Event;
use tracing::{error, info};

cfg_if! {
    if #[cfg(unix)] {
        mod unix;
        pub use unix::*;
    } else if #[cfg(windows)] {
        mod windows;
        pub use windows::*;
    } else {
        // not supported
    }
}

pub static INPUT_BUFFER: RwLock<String> = RwLock::new(String::new());

pub const PROMPT: &[u8] = b">> ";

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
                    if INPUT_BUFFER.write()?.pop().is_some() {
                        let mut stdout = stdout().lock();
                        stdout.write_all(&[8, b' ', 8])?;
                        stdout.flush()?;
                    };
                }
                KeyCode::Enter => {
                    let input = {
                        let mut wl = INPUT_BUFFER.write()?;
                        let s = wl.clone();
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
                        "help" | "?" | "h" => {
                            static INFOS: &[&str] = &["help: 显示本帮助", "exit: 退出程序"];

                            let mut s = String::from('\n');
                            for &info in INFOS {
                                s.push_str(info);
                                s.push('\n');
                            }
                            s.pop();
                            info!("{}", s);
                        }
                        "exit" | "quit" | "stop" | "e" => {
                            info!("正在停止AtriBot");
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
