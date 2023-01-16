pub(crate) mod buffer;
pub use buffer::{
    enter_alternate_screen, exit_alternate_screen, is_alternate_screen_enabled, is_terminal_closed,
};

mod sys;

pub use sys::handle_standard_output;

use crate::service::command::{builtin::handle_plugin_command, PLUGIN_COMMAND};
use crate::terminal::buffer::{INPUT_BUFFER, INPUT_CACHE, TERMINAL_CLOSED};
use crate::PluginManager;
use crossterm::cursor::MoveToColumn;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{event, execute};
use event::Event;
use std::error::Error;
use std::io::{stdout, Write};
use std::sync::atomic::Ordering;
use tracing::{error, info};

pub const BUFFER_SIZE: usize = 512;

pub const PROMPT: &[u8] = b">> ";

pub fn stop_info() {
    info!("正在停止AtriBot");
}

pub fn start_read_input(manager: &mut PluginManager) -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;

    INPUT_BUFFER.write()?.reserve(BUFFER_SIZE);

    execute!(stdout(), EnableBracketedPaste)?;

    loop {
        let e = event::read()?;

        match e {
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => {
                stop_info();
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => {
                execute!(stdout(), EnterAlternateScreen)?;
                loop {
                    match event::read()? {
                        Event::Key(KeyEvent {
                            code: KeyCode::Char('q'),
                            modifiers: KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            state: _,
                        }) => {
                            break;
                        }
                        _ => {}
                    }
                }
                execute!(stdout(), LeaveAlternateScreen)?;
            }
            Event::Key(KeyEvent {
                code: k @ (KeyCode::Up | KeyCode::Down),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => {
                fn output(buffer: &mut String, output: &str) -> std::io::Result<()> {
                    let mut stdout = stdout().lock();
                    execute!(stdout, Clear(ClearType::CurrentLine), MoveToColumn(0))?;
                    stdout.write_all(PROMPT)?;
                    stdout.write_all(output.as_bytes())?;
                    stdout.flush()?;

                    buffer.clear();
                    buffer.push_str(output);

                    Ok(())
                }

                let mut cache = INPUT_CACHE.lock()?;
                let mut buffer = INPUT_BUFFER.write()?;
                match k {
                    KeyCode::Up => {
                        let index = cache.index;
                        let len = cache.caches.len();
                        if index == 0 {
                            if let Some(str) = cache.caches.get(0) {
                                output(&mut buffer, str)?;
                            }
                        } else if index <= len {
                            cache.index -= 1;

                            if index == len {
                                cache.last_input.clear();
                                cache.last_input.push_str(&buffer);
                            }

                            output(&mut buffer, &cache.caches[cache.index])?;
                        }
                    }
                    KeyCode::Down => {
                        cache.index += 1;
                        let index = cache.index;
                        let len = cache.caches.len();
                        if index < len {
                            output(&mut buffer, &cache.caches[cache.index])?;
                        } else if index >= len {
                            cache.index = len;
                            output(&mut buffer, &cache.last_input)?;
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Event::Key(KeyEvent {
                code,
                modifiers: _,
                kind: KeyEventKind::Press,
                state: _,
            }) => match code {
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
                    let mut wl = INPUT_BUFFER.write()?;

                    let mut stdout = stdout().lock();
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;

                    let cmd = wl.trim_end();
                    match cmd {
                        "" => {
                            stdout.write_all(PROMPT)?;
                            stdout.flush()?;
                            wl.clear();
                            continue;
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
                        "exit" | "quit" | "stop" | "q" => {
                            TERMINAL_CLOSED.store(true, Ordering::Relaxed);
                            break;
                        }
                        plugin if plugin.starts_with(PLUGIN_COMMAND) => {
                            if let Err(e) = handle_plugin_command(plugin, manager) {
                                error!("{}", e);
                            }
                        }
                        or => {
                            info!("未知的命令 '{}', 使用 'help' 显示帮助信息", or);
                        }
                    }

                    {
                        let mut cache = INPUT_CACHE.lock()?;
                        cache.caches.push(String::from(&*wl));
                        cache.index = cache.caches.len();
                        cache.last_input.clear();
                    }

                    wl.clear();
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

    execute!(
        stdout(),
        DisableBracketedPaste,
        Clear(ClearType::CurrentLine),
        MoveToColumn(0)
    )?;

    disable_raw_mode()?;
    Ok(())
}
