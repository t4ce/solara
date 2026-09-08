//! Small immediate terminal navigator using the same lease/raw mode as Texplo.
use crossterm::{
    cursor::{MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute, queue,
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
            DisableLineWrap,
            EnableMouseCapture,
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
        self.bookmarks.editing = false;
        match target {
            NavigationTarget::Web(url) => self.address.set_url(url),
            NavigationTarget::Demo(demo) => self.address.set_demo(*demo),
        }
        self.dirty = true;
    }
    fn field_width(&self) -> usize {
        self.columns.saturating_sub(19).max(1) as usize
    }
    fn paint(&mut self) -> io::Result<()> {
        if !self.dirty || self.lease.is_none() {
            return Ok(());
        }
        let width = self.field_width();
        let cursor = self.address.text[..self.address.cursor].chars().count();
        let start = cursor.saturating_sub(width.saturating_sub(1));
        let text: String = self.address.text.chars().skip(start).take(width).collect();
        let selected = self.address.selected;
        let protocol = if self.address.http { "HTTP " } else { "HTTPS" };
        let mut out = io::BufWriter::new(io::stdout());
        queue!(out, MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
        write!(
            out,
            "Solara  [{}]",
            if self.bookmarks.editing {
                "editing address"
            } else {
                "1–9 bookmarks"
            }
        )?;
        queue!(out, MoveTo(0, 1), Clear(ClearType::CurrentLine))?;
        write!(
            out,
            "[{protocol}] URL [{}{:width$}\x1b[0m]",
            if selected { "\x1b[7m" } else { "" },
            text
        )?;
        queue!(out, MoveTo(0, 2), Clear(ClearType::CurrentLine))?;
        let status: String = self
            .status
            .chars()
            .filter(|c| !c.is_control())
            .take(self.columns as usize)
            .collect();
        write!(out, "{status}")?;
        queue!(out, MoveTo(0, 3), Clear(ClearType::CurrentLine))?;
        let help: String =
            "1-9: pick  Enter: go  Ctrl-L: edit  Tab: picks  F2: HTTP/S  Esc: Shell  Ctrl-Q: quit"
                .chars()
                .take(self.columns as usize)
                .collect();
        write!(out, "{help}")?;
        for (index, (label, target)) in self.bookmarks.entries.iter().enumerate() {
            let row = 4 + index as u16;
            if row >= self.rows {
                break;
            }
            queue!(out, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
            let line: String = format!("{}  {label}  {target}", index + 1)
                .chars()
                .take(self.columns as usize)
                .collect();
            write!(out, "{line}")?;
        }
        let x = 13 + cursor - start;
        queue!(
            out,
            MoveTo((x as u16).min(self.columns.saturating_sub(1)), 1),
            Show
        )?;
        out.flush()?;
        self.dirty = false;
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
                    self.bookmarks.editing = true;
                    self.address.insert(text.trim());
                    self.dirty = true;
                }
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left) && mouse.row == 1 =>
                {
                    self.bookmarks.editing = true;
                    if mouse.column < 7 {
                        self.address.toggle();
                    } else {
                        self.address.cursor = self.address.text.len();
                        self.address.selected = false;
                    }
                    self.dirty = true;
                }
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left) && mouse.row >= 4 =>
                {
                    if self
                        .bookmarks
                        .pick((mouse.row - 4) as usize, &mut self.address)
                    {
                        self.status("Bookmark selected — Enter to go, Ctrl-L to edit");
                    }
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
                        self.bookmarks.editing = true;
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
                        KeyCode::Tab => self.bookmarks.editing = false,
                        KeyCode::Char('l') if ctrl => {
                            self.bookmarks.editing = true;
                            self.address.select_all();
                        }
                        KeyCode::F(2) => self.address.toggle(),
                        KeyCode::Enter => match self.address.target() {
                            Ok(target) => {
                                self.location(&target);
                                action = Some(Action::Navigate(target));
                                break;
                            }
                            Err(error) => self.status(error),
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
                            if self.bookmarks.character(c, &mut self.address) {
                                self.status("Bookmark selected — Enter to go, Ctrl-L to edit");
                            }
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
