use super::{
    args::{Cli, Command, ConfigCommand, ServiceCommand},
    management::{self, Backend},
    render,
};
use atri_bot::{Runtime, control::ControlClient};
use atri_core::{ApiError, RuntimeNotice, RuntimeSnapshot};
use clap::{CommandFactory, Parser, error::ErrorKind};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::{
    collections::VecDeque,
    io::{self, Stdout},
    path::Path,
    time::Duration,
};

const LOG_LINES: usize = 2000;
const LINE_BYTES: usize = 8192;
const INPUT_BYTES: usize = 16384;
const HISTORY_ENTRIES: usize = 100;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Stop,
}

#[derive(Default)]
struct Input {
    text: String,
    // UTF-8 byte boundary, never a terminal column or arbitrary byte offset.
    caret: usize,
    history: VecDeque<String>,
    history_at: Option<usize>,
    draft: String,
}
impl Input {
    fn insert(&mut self, text: &str) {
        let remaining = INPUT_BYTES - self.text.len();
        if text.len() <= remaining && !text.chars().any(char::is_control) {
            self.text.insert_str(self.caret, text);
            self.caret += text.len();
            return;
        }
        let mut clean = String::with_capacity(text.len().min(remaining));
        for ch in text.chars() {
            let ch = if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            };
            if ch.is_control() {
                continue;
            }
            if clean.len() + ch.len_utf8() > remaining {
                break;
            }
            clean.push(ch);
        }
        self.text.insert_str(self.caret, &clean);
        self.caret += clean.len();
    }
    fn left(&mut self) {
        self.caret = self.text[..self.caret]
            .char_indices()
            .next_back()
            .map_or(0, |(at, _)| at);
    }
    fn right(&mut self) {
        if let Some(ch) = self.text[self.caret..].chars().next() {
            self.caret += ch.len_utf8();
        }
    }
    fn backspace(&mut self) {
        let end = self.caret;
        self.left();
        self.text.drain(self.caret..end);
    }
    fn delete(&mut self) {
        if let Some(ch) = self.text[self.caret..].chars().next() {
            self.text.drain(self.caret..self.caret + ch.len_utf8());
        }
    }
    fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.history_at = None;
        self.draft.clear();
    }
    fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.clear();
        text
    }
    fn remember(&mut self, text: String) {
        if self.history.back() != Some(&text) {
            self.history.push_back(text);
            if self.history.len() > HISTORY_ENTRIES {
                self.history.pop_front();
            }
        }
    }
    fn history(&mut self, older: bool) {
        let at = match (older, self.history_at) {
            (_, _) if self.history.is_empty() => return,
            (true, None) => {
                self.draft = self.text.clone();
                Some(self.history.len() - 1)
            }
            (true, Some(at)) => Some(at.saturating_sub(1)),
            (false, Some(at)) if at + 1 < self.history.len() => Some(at + 1),
            (false, Some(_)) => None,
            (false, None) => return,
        };
        self.text = at.map_or_else(|| self.draft.clone(), |at| self.history[at].clone());
        self.caret = self.text.len();
        self.history_at = at;
    }
    fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let mut bytes = [0; 4];
                self.insert(ch.encode_utf8(&mut bytes));
            }
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.caret = 0,
            KeyCode::End => self.caret = self.text.len(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Up => self.history(true),
            KeyCode::Down => self.history(false),
            KeyCode::Esc => self.clear(),
            _ => {}
        }
    }
    /// Keep the caret visible without slicing inside a Unicode scalar.
    fn viewport(&self, columns: u16) -> (&str, u16) {
        let columns = usize::from(columns.saturating_sub(1));
        let mut start = 0;
        while Span::raw(&self.text[start..self.caret]).width() > columns {
            start += self.text[start..].chars().next().unwrap().len_utf8();
        }
        let column = Span::raw(&self.text[start..self.caret]).width() as u16;
        (&self.text[start..], column)
    }
}

enum Prompt {
    Empty,
    Output(String),
    Stop,
    Command(Command),
}

fn help() -> String {
    format!(
        "{}\nConsole: quit / exit / Ctrl-C gracefully stop extensions and exit.\nEnter executes; Left/Right/Home/End edit; Up/Down history; PageUp/PageDown scroll.\nEsc dismisses interaction or clears input; F1 shows help. Paste is single-line.\nSensitive commands are not echoed or retained in history; history is memory-only.\n",
        Cli::command().render_long_help()
    )
}

fn parse_prompt(input: &str) -> Result<Prompt, ApiError> {
    let words = shell_words::split(input).map_err(|_| {
        ApiError::new(
            "invalid_command",
            "Unclosed quote or escape in command; type help for usage",
        )
    })?;
    if words.is_empty() {
        return Ok(Prompt::Empty);
    }
    if words.len() == 1 {
        match words[0].as_str() {
            "quit" | "exit" => return Ok(Prompt::Stop),
            "help" | "?" => return Ok(Prompt::Output(help())),
            _ => {}
        }
    }
    let cli = match Cli::try_parse_from(std::iter::once("atri".to_owned()).chain(words)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return Ok(Prompt::Output(error.to_string()));
        }
        // clap diagnostics can quote an unknown argument containing private data.
        Err(_) => {
            return Err(ApiError::new(
                "invalid_command",
                "Invalid command or arguments; type help or COMMAND --help for usage",
            ));
        }
    };
    if cli.bot_dir.is_some() {
        return Err(ApiError::new(
            "active_directory",
            "The console cannot change --bot-dir; it manages the active runtime",
        ));
    }
    Ok(cli.command.map_or(Prompt::Empty, Prompt::Command))
}

fn retain_history(command: &Command) -> bool {
    !matches!(
        command,
        Command::Config {
            command: ConfigCommand::Set { .. } | ConfigCommand::Import { .. }
        } | Command::Services {
            command: ServiceCommand::Call { .. }
        }
    )
}

pub struct Dashboard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
    snapshot: RuntimeSnapshot,
    logs: VecDeque<String>,
    input: Input,
    scroll: usize,
    modal: Option<String>,
    modal_scroll: u16,
    pending: Option<tokio::task::JoinHandle<super::CliResult<String>>>,
}
impl Dashboard {
    pub fn open() -> Result<Self, ApiError> {
        terminal::enable_raw_mode().map_err(ApiError::internal)?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste) {
            let _ = execute!(stdout, DisableBracketedPaste, LeaveAlternateScreen);
            let _ = terminal::disable_raw_mode();
            return Err(ApiError::internal(error));
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
                let _ = terminal::disable_raw_mode();
                return Err(ApiError::internal(error));
            }
        };
        let mut view = Self {
            terminal,
            active: true,
            snapshot: RuntimeSnapshot {
                extensions: Vec::new(),
                running: false,
            },
            logs: VecDeque::new(),
            input: Input::default(),
            scroll: 0,
            modal: None,
            modal_scroll: 0,
            pending: None,
        };
        view.push("ATRI event console · type help for commands; quit or exit to stop.".into());
        Ok(view)
    }
    fn suspend(&mut self) -> Result<(), ApiError> {
        if !self.active {
            return Ok(());
        }
        let cursor = self.terminal.show_cursor();
        let leave = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let raw = terminal::disable_raw_mode();
        self.active = false;
        cursor.and(leave).and(raw).map_err(ApiError::internal)
    }
    fn resume(&mut self) -> Result<(), ApiError> {
        terminal::enable_raw_mode().map_err(ApiError::internal)?;
        if let Err(error) = execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste
        ) {
            let _ = execute!(
                self.terminal.backend_mut(),
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
            let _ = terminal::disable_raw_mode();
            return Err(ApiError::internal(error));
        }
        self.active = true;
        self.terminal.clear().map_err(ApiError::internal)
    }
    fn push(&mut self, text: String) {
        for line in text.lines() {
            let mut line = render::clean(line);
            if line.len() > LINE_BYTES {
                let mut end = LINE_BYTES;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                line.truncate(end);
                line.push_str(" … [truncated]");
            }
            self.logs.push_back(line);
            if self.scroll > 0 {
                self.scroll = self.scroll.saturating_add(1);
            }
            if self.logs.len() > LOG_LINES {
                self.logs.pop_front();
            }
        }
    }
    pub fn notice(&mut self, notice: &RuntimeNotice) {
        match notice {
            RuntimeNotice::Interaction { request, .. } => {
                // QR and challenge content never enter scrollback or command history.
                self.modal = Some(render::interaction(request));
                self.modal_scroll = 0;
            }
            RuntimeNotice::Table { extension, table } => self.push(format!(
                "{}: {}",
                render::clean(extension),
                render::output_table(table)
            )),
            RuntimeNotice::Log { extension, record } => self.push(format!(
                "[{:?}] {}: {}",
                record.level,
                render::clean(extension),
                render::clean(&record.message)
            )),
            RuntimeNotice::Event(event) => self.push(render::event(event)),
            RuntimeNotice::State(info) => self.push(format!(
                "{}: {}{}",
                render::clean(&info.metadata.id),
                render::state(&info.state),
                info.error
                    .as_ref()
                    .map(|e| format!(" — {}", render::clean(e)))
                    .unwrap_or_default()
            )),
        }
    }
    pub fn lagged(&mut self, count: u64) {
        self.push(format!(
            "Notice stream lagged: {count} notices skipped; use status for current state."
        ));
    }
    async fn submit(&mut self, bot_dir: &Path) -> Result<Action, ApiError> {
        let text = self.input.take();
        let prompt = match parse_prompt(&text) {
            Ok(prompt) => prompt,
            Err(error) => {
                self.push(format!("Error: {}", render::clean(&error.to_string())));
                return Ok(Action::Continue);
            }
        };
        match prompt {
            Prompt::Empty => {}
            Prompt::Output(output) => self.push(output),
            Prompt::Stop => return Ok(Action::Stop),
            Prompt::Command(command) => {
                if self.pending.is_some() {
                    self.push(
                        "A command is still running; quit or Ctrl-C can stop the runtime.".into(),
                    );
                    return Ok(Action::Continue);
                }
                if retain_history(&command) {
                    self.input.remember(text);
                }
                let editing = matches!(
                    &command,
                    Command::Config {
                        command: ConfigCommand::Edit { .. }
                    }
                );
                let bot_dir = bot_dir.to_owned();
                let command = async move {
                    let client = ControlClient::connect(&bot_dir).await?.ok_or_else(|| {
                        ApiError::new(
                            "runtime_unavailable",
                            "Runtime control endpoint is unavailable",
                        )
                    })?;
                    management::dispatch(&Backend::Remote(client), command).await
                };
                if editing {
                    if let Err(error) = self.suspend() {
                        self.resume()?;
                        self.push(format!("Error: {}", render::clean(&error.to_string())));
                        return Ok(Action::Continue);
                    }
                    let output = command.await;
                    self.resume()?;
                    self.command_finished(output);
                } else {
                    self.pending = Some(tokio::spawn(command));
                }
            }
        }
        Ok(Action::Continue)
    }
    fn command_finished(&mut self, output: super::CliResult<String>) {
        match output {
            Ok(output) => {
                self.scroll = 0;
                self.push(output);
            }
            Err(error) => self.push(format!(
                "Error: {}",
                render::clean(&error.error.to_string())
            )),
        }
    }
    fn key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Stop;
        }
        if self.modal.is_some() {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.modal = None,
                KeyCode::Down | KeyCode::PageDown => {
                    self.modal_scroll = self.modal_scroll.saturating_add(3)
                }
                KeyCode::Up | KeyCode::PageUp => {
                    self.modal_scroll = self.modal_scroll.saturating_sub(3)
                }
                _ => {}
            }
            return Action::Continue;
        }
        match key.code {
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(10).min(LOG_LINES * LINE_BYTES)
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::F(1) => self.push(help()),
            _ => self.input.key(key),
        }
        Action::Continue
    }
    pub async fn tick(&mut self, runtime: &Runtime, bot_dir: &Path) -> Result<Action, ApiError> {
        self.snapshot = runtime.snapshot().await;
        if self
            .pending
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            let output = self
                .pending
                .take()
                .unwrap()
                .await
                .map_err(ApiError::internal)?;
            self.command_finished(output);
        }
        // Bound work per tick so a paste/key flood cannot starve runtime notices.
        for _ in 0..128 {
            if !event::poll(Duration::ZERO).map_err(ApiError::internal)? {
                break;
            }
            match event::read().map_err(ApiError::internal)? {
                Event::Paste(text) if self.modal.is_none() => self.input.insert(&text),
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let action = if key.code == KeyCode::Enter && self.modal.is_none() {
                        self.submit(bot_dir).await?
                    } else {
                        self.key(key)
                    };
                    if action != Action::Continue {
                        return Ok(action);
                    }
                }
                _ => {}
            }
        }
        self.draw(bot_dir)?;
        Ok(Action::Continue)
    }
    fn draw(&mut self, bot_dir: &Path) -> Result<(), ApiError> {
        let Self {
            terminal,
            snapshot,
            logs,
            input,
            scroll,
            modal,
            modal_scroll,
            ..
        } = self;
        terminal
            .draw(|frame| {
                let areas = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(frame.area());
                frame.render_widget(
                    Paragraph::new(format!(
                        " ATRI · {} · {} extensions · {}",
                        if snapshot.running {
                            "running"
                        } else {
                            "stopping"
                        },
                        snapshot.extensions.len(),
                        render::clean(&bot_dir.display().to_string())
                    ))
                    .style(Style::default().fg(Color::Cyan)),
                    areas[0],
                );
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Events / logs / command output · PageUp/PageDown scroll ");
                let content = block.inner(areas[1]);
                // Wrap into borrowed slices ourselves so scrolling uses actual displayed rows.
                let visible = visible_lines(logs, content.width, content.height as usize, scroll)
                    .into_iter()
                    .map(Line::raw)
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(Text::from(visible)).block(block), areas[1]);
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Command · help · quit / exit ");
                let content = block.inner(areas[2]);
                let (text, column) = input.viewport(content.width);
                frame.render_widget(Paragraph::new(text).block(block), areas[2]);
                if content.width > 0 && content.height > 0 && modal.is_none() {
                    frame.set_cursor_position((content.x + column, content.y));
                }
                if let Some(request) = modal {
                    let area = centered(frame.area(), 90, 90);
                    frame.render_widget(Clear, area);
                    frame.render_widget(
                        Paragraph::new(request.as_str())
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_style(Style::default().fg(Color::Cyan))
                                    .title(" Interaction · Enter/Esc dismiss · ↑↓ scroll "),
                            )
                            .scroll((*modal_scroll, 0))
                            .wrap(Wrap { trim: false }),
                        area,
                    );
                }
            })
            .map_err(ApiError::internal)?;
        Ok(())
    }
}

fn wrap_line<'a>(line: &'a str, width: u16, rows: &mut Vec<&'a str>) {
    let mut start = 0;
    let mut columns = 0;
    for (at, ch) in line.char_indices() {
        let mut bytes = [0; 4];
        let size = Span::raw(&*ch.encode_utf8(&mut bytes)).width();
        if columns + size > usize::from(width) && at > start {
            rows.push(&line[start..at]);
            start = at;
            columns = 0;
        }
        columns += size;
    }
    rows.push(&line[start..]);
}

/// Only wrap the tail needed by the viewport, rather than the entire history per frame.
fn visible_lines<'a>(
    logs: &'a VecDeque<String>,
    width: u16,
    height: usize,
    scroll: &mut usize,
) -> Vec<&'a str> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut visible = Vec::with_capacity(height);
    let mut wrapped = Vec::new();
    let mut skip = *scroll;
    let mut seen: usize = 0;
    for line in logs.iter().rev() {
        wrapped.clear();
        wrap_line(line, width, &mut wrapped);
        for row in wrapped.iter().rev() {
            seen += 1;
            if skip > 0 {
                skip -= 1;
            } else {
                visible.push(*row);
            }
            if visible.len() == height {
                visible.reverse();
                return visible;
            }
        }
    }
    let max_scroll = seen.saturating_sub(height);
    if *scroll > max_scroll {
        *scroll = max_scroll;
        return visible_lines(logs, width, height, scroll);
    }
    visible.reverse();
    visible
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - height) / 2),
        Constraint::Percentage(height),
        Constraint::Percentage((100 - height) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - width) / 2),
        Constraint::Percentage(width),
        Constraint::Percentage((100 - width) / 2),
    ])
    .split(vertical[1])[1]
}
impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(command) = self.pending.take() {
            command.abort();
        }
        let _ = self.suspend();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_uses_real_cli_parser_and_preserves_quoted_values() {
        let Prompt::Command(Command::Config {
            command: ConfigCommand::Set { id, key, value },
        }) = parse_prompt("config set example /name '\"你好 world\"'").unwrap()
        else {
            panic!("config set")
        };
        assert_eq!(
            (id.as_str(), key.as_str(), value.as_str()),
            ("example", "/name", "\"你好 world\"")
        );
        for command in [
            "status",
            "plugins list",
            "config get example",
            "tasks list",
            "services list",
            "stop",
        ] {
            assert!(matches!(parse_prompt(command).unwrap(), Prompt::Command(_)));
        }
        assert!(matches!(parse_prompt("  ").unwrap(), Prompt::Empty));
        assert!(matches!(parse_prompt("quit").unwrap(), Prompt::Stop));
        assert!(matches!(parse_prompt("exit").unwrap(), Prompt::Stop));
        assert!(matches!(
            parse_prompt("plugins --help").unwrap(),
            Prompt::Output(_)
        ));
        assert_eq!(
            parse_prompt("status --bot-dir other").err().unwrap().code,
            "active_directory"
        );
        assert!(parse_prompt("quit extra").is_err());
    }
    #[test]
    fn parser_errors_and_sensitive_history_do_not_echo_arguments() {
        let error = parse_prompt("config set example /password private-secret extra")
            .err()
            .unwrap();
        assert!(!error.message.contains("private-secret"));
        for text in [
            "config set example /password '\"private-secret\"'",
            "services call signer sign --json '{\"secret\":1}'",
        ] {
            let Prompt::Command(command) = parse_prompt(text).unwrap() else {
                panic!("command")
            };
            assert!(!retain_history(&command));
        }
    }
    #[test]
    fn unicode_caret_edits_and_paste_remain_single_line() {
        let mut input = Input::default();
        input.insert("a你🦀e\u{301}");
        input.left();
        input.backspace();
        assert_eq!(input.text, "a你🦀\u{301}");
        input.caret = 1;
        input.delete();
        input.insert("界\n\x1b\t");
        assert_eq!(input.text, "a界  🦀\u{301}");
        assert!(input.text.is_char_boundary(input.caret));
        let (_, col) = input.viewport(4);
        assert!(col < 4);
        input.caret = 0;
        input.backspace();
        assert_eq!(input.caret, 0);
        input.caret = input.text.len();
        input.delete();
    }
    #[test]
    fn history_is_bounded_and_restores_unsubmitted_draft() {
        let mut input = Input::default();
        for i in 0..HISTORY_ENTRIES + 5 {
            input.remember(format!("plugins info p{i}"));
        }
        assert_eq!(input.history.len(), HISTORY_ENTRIES);
        input.insert("unfinished 你好");
        input.history(true);
        assert_eq!(input.text, "plugins info p104");
        input.history(false);
        assert_eq!(input.text, "unfinished 你好");
        input.insert(&"🦀".repeat(INPUT_BYTES));
        assert!(input.text.len() <= INPUT_BYTES);
        assert!(input.text.is_char_boundary(input.caret));
    }
    #[test]
    fn former_tab_shortcuts_are_editable_command_characters() {
        let mut input = Input::default();
        for ch in "qer123".chars() {
            input.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert_eq!(input.text, "qer123");
        input.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.text, "er12");
        input.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!((input.text.as_str(), input.caret), ("", 0));
    }
    #[test]
    fn visual_scrollback_wraps_unicode_without_invalid_slices() {
        let lines = VecDeque::from(["a你好b".to_owned(), "".to_owned()]);
        let mut scroll = 0;
        assert_eq!(visible_lines(&lines, 3, 3, &mut scroll), ["a你", "好b", ""]);
        assert_eq!(visible_lines(&lines, 3, 2, &mut scroll), ["好b", ""]);
        scroll = 100;
        assert_eq!(visible_lines(&lines, 3, 2, &mut scroll), ["a你", "好b"]);
        assert_eq!(scroll, 1);
        assert!(visible_lines(&lines, 0, 3, &mut scroll).is_empty());
    }
}
