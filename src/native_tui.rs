//! Small immediate terminal navigator using the same lease/raw mode as Texplo.
use crossterm::{
    cursor::{MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, Event, KeyCode,
        KeyEventKind, KeyModifiers,
    },
    execute, queue,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use solara::navigation::{Address, Bookmarks, NavigationTarget};
use std::{
    io::{self, Write},
    time::Duration,
};
use trueos::terminal_style::{CONTROL_BACKGROUND_RGB, CONTROL_ROWS};
use trueos::vshell::{TerminalLease, TerminalParkingTicket, TerminalReentry};

pub enum Action {
    Navigate(NavigationTarget),
    Quit,
}
pub struct Navigator {
    address: Address,
    bookmarks: Bookmarks,
    rows: u16,
    status: String,
    columns: u16,
    dirty: bool,
    lease: Option<TerminalLease>,
    parked: Option<TerminalParkingTicket>,
    raw: bool,
    fresh_input: bool,
}
fn io_error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
impl Navigator {
    pub fn new() -> io::Result<Self> {
        let lease = trueos::vshell::terminal_initial_lease().map_err(io_error)?;
        let (bookmarks, status) =
            match trueos::async_fs::block_on(trueos::async_fs::read_file_utf8(b"vFile:startup")) {
                Ok(json) => match Bookmarks::from_startup(&json) {
                    Ok(list) => (list, "Opening home…".into()),
                    Err(e) => (Bookmarks::default(), format!("Bookmarks: {e}")),
                },
                Err(_) => (
                    Bookmarks::default(),
                    "Opening home… (no startup bookmarks)".into(),
                ),
            };
        let mut ui = Self {
            address: Address::default(),
            bookmarks,
            rows: 24,
            status,
            columns: 80,
            dirty: true,
            lease: Some(lease),
            parked: None,
            raw: false,
            fresh_input: true,
        };
        ui.enter()?;
        Ok(ui)
    }
    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.raw = true;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            ResetColor,
            DisableLineWrap,
            DisableMouseCapture,
            EnableBracketedPaste,
            Clear(ClearType::All),
            Show
        )?;
        (self.columns, self.rows) = terminal::size()?;
        self.dirty = true;
        self.paint()?;
        if let Some(lease) = &self.lease {
            lease.acknowledge_ready().map_err(io_error)?;
        }
        Ok(())
    }
    fn restore(&mut self) -> io::Result<()> {
        if !self.raw {
            return Ok(());
        }
        let screen = execute!(
            io::stdout(),
            Show,
            DisableBracketedPaste,
            DisableMouseCapture,
            EnableLineWrap,
            ResetColor,
            LeaveAlternateScreen
        );
        let raw = terminal::disable_raw_mode();
        self.raw = false;
        screen.and(raw)
    }
    pub fn status(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.dirty = true;
    }
    pub fn location(&mut self, target: &NavigationTarget) {
        self.fresh_input = true;
        match target {
            NavigationTarget::Web(url) => self.address.set_url(url),
            NavigationTarget::Demo(demo) => self.address.set_demo(*demo),
        }
        self.dirty = true;
    }
    fn field_width(&self) -> usize {
        self.columns
            .saturating_sub(if self.address.http { 7 } else { 8 })
            .max(1) as usize
    }
    fn paint(&mut self) -> io::Result<()> {
        if !self.dirty || self.lease.is_none() {
            return Ok(());
        }
        let mut out = io::BufWriter::new(io::stdout());
        self.render(&mut out)?;
        out.flush()?;
        self.dirty = false;
        Ok(())
    }
    fn render(&self, out: &mut impl Write) -> io::Result<()> {
        let width = self.field_width();
        let cursor = self.address.text[..self.address.cursor].chars().count();
        let start = cursor.saturating_sub(width.saturating_sub(1));
        let text: String = self.address.text.chars().skip(start).take(width).collect();
        let protocol = if self.address.http { "HTTP" } else { "HTTPS" };
        let prefix = format!("[{protocol}] ");
        let [r, g, b] = CONTROL_BACKGROUND_RGB;
        queue!(out, ResetColor, SetBackgroundColor(Color::Rgb { r, g, b }))?;
        for row in 0..CONTROL_ROWS.min(self.rows) {
            queue!(out, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        }
        if self.rows > 1 {
            queue!(out, MoveTo(0, 1))?;
            let prefix: String = prefix.chars().take(self.columns as usize).collect();
            write!(out, "{prefix}")?;
            if prefix.chars().count() < self.columns as usize {
                write!(
                    out,
                    "{}{text}\x1b[27m",
                    if self.address.selected { "\x1b[7m" } else { "" }
                )?;
            }
        }
        queue!(out, SetBackgroundColor(Color::Reset))?;
        let column_width = (self.columns as usize).saturating_sub(4) / 2;
        for row in CONTROL_ROWS..self.rows {
            queue!(out, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
            let index = (row - CONTROL_ROWS) as usize;
            if let Some((label, target)) = self.bookmarks.entries.get(index).filter(|_| index < 99)
            {
                let label: String = label.chars().take(column_width).collect();
                let target: String = target.to_string().chars().take(column_width).collect();
                let line = format!(
                    "{:>2} {label:column_width$} {target:column_width$}",
                    index + 1
                );
                write!(
                    out,
                    "{}",
                    line.chars().take(self.columns as usize).collect::<String>()
                )?;
            }
        }
        if self.rows > 1 {
            let x = prefix.chars().count() + cursor - start;
            queue!(
                out,
                MoveTo((x as u16).min(self.columns.saturating_sub(1)), 1),
                Show
            )?;
        }
        Ok(())
    }
    pub fn tick(&mut self) -> io::Result<Option<Action>> {
        if let Some(ticket) = &self.parked {
            match ticket.poll_reentry().map_err(io_error)? {
                TerminalReentry::Pending => return Ok(None),
                TerminalReentry::Ready(lease) => {
                    self.parked = None;
                    self.lease = Some(lease);
                    self.enter()?;
                }
            }
        }
        let mut action = None;
        for _ in 0..64 {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            match event::read()? {
                Event::Resize(columns, rows) => {
                    self.rows = rows;
                    self.columns = columns;
                    self.dirty = true;
                }
                Event::Paste(text) => {
                    if self.fresh_input {
                        self.address.select_all();
                    }
                    self.fresh_input = false;
                    self.address.insert(text.trim());
                    self.dirty = true;
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    if matches!(
                        key.code,
                        KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Home
                            | KeyCode::End
                            | KeyCode::Backspace
                            | KeyCode::Delete
                    ) {
                        self.fresh_input = false;
                    }
                    match key.code {
                        KeyCode::Char('q') if ctrl => {
                            action = Some(Action::Quit);
                            break;
                        }
                        KeyCode::Esc => {
                            self.restore()?;
                            if let Some(lease) = self.lease.take() {
                                self.parked = Some(lease.release_to_shell().map_err(io_error)?);
                            }
                            return Ok(None);
                        }
                        KeyCode::Tab => {
                            self.fresh_input = true;
                            self.address.select_all();
                        }
                        KeyCode::Char('l') if ctrl => {
                            self.fresh_input = false;
                            self.address.select_all();
                        }
                        KeyCode::F(2) => self.address.toggle(),
                        KeyCode::Enter => match self.bookmarks.target(&self.address) {
                            Some(target) => {
                                self.location(&target);
                                action = Some(Action::Navigate(target));
                                break;
                            }
                            None => {}
                        },
                        KeyCode::Left => self.address.left(),
                        KeyCode::Right => self.address.right(),
                        KeyCode::Home => {
                            self.address.cursor = 0;
                            self.address.selected = false;
                        }
                        KeyCode::End => {
                            self.address.cursor = self.address.text.len();
                            self.address.selected = false;
                        }
                        KeyCode::Backspace => self.address.backspace(),
                        KeyCode::Delete => self.address.delete(),
                        KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                            if self.fresh_input {
                                self.address.select_all();
                            }
                            self.fresh_input = false;
                            self.address.insert(&c.to_string());
                        }
                        _ => {}
                    }
                    self.dirty = true;
                }
                _ => {}
            }
        }
        self.paint()?;
        Ok(action)
    }
}
impl Drop for Navigator {
    fn drop(&mut self) {
        let _ = self.restore();
        if let Some(lease) = self.lease.take() {
            let _ = lease.release_to_shell();
        }
    }
}
