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
use solara::navigation::Address;
use std::{
    io::{self, Write},
    time::Duration,
};
use trueos::vshell::{TerminalLease, TerminalParkingTicket, TerminalReentry};
use url::Url;

pub enum Action {
    Navigate(Url),
    Quit,
}
pub struct Navigator {
    address: Address,
    protocol_focus: bool,
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
        let mut ui = Self {
            address: Address::default(),
            protocol_focus: false,
            status: "Opening home…".into(),
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
        self.columns = terminal::size()?.0;
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
    pub fn location(&mut self, url: &Url) {
        self.address.set_url(url);
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
        let selected = self.address.selected && !self.protocol_focus;
        let protocol = if self.address.http { "HTTP " } else { "HTTPS" };
        let mut out = io::BufWriter::new(io::stdout());
        queue!(out, MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
        write!(out, "Solara")?;
        queue!(out, MoveTo(0, 1), Clear(ClearType::CurrentLine))?;
        write!(
            out,
            "URL [{}{:width$}\x1b[0m] {}[{protocol}]\x1b[0m",
            if selected { "\x1b[7m" } else { "" },
            text,
            if self.protocol_focus { "\x1b[7m" } else { "" }
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
            "Enter: go  Tab: protocol  F2: toggle  Ctrl-L: address  Esc: Shell2  Ctrl-Q: quit"
                .chars()
                .take(self.columns as usize)
                .collect();
        write!(out, "{help}")?;
        let x = if self.protocol_focus {
            width + 8
        } else {
            5 + cursor - start
        };
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
                Event::Resize(columns, _) => {
                    self.columns = columns;
                    self.dirty = true;
                }
                Event::Paste(text) => {
                    self.address.insert(text.trim());
                    self.protocol_focus = false;
                    self.dirty = true;
                }
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left) && mouse.row == 1 =>
                {
                    self.protocol_focus = mouse.column as usize >= self.field_width() + 7;
                    if self.protocol_focus {
                        self.address.toggle();
                    } else {
                        self.address.cursor = self.address.text.len();
                        self.address.selected = false;
                    }
                    self.dirty = true;
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
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
                        KeyCode::Char('l') if ctrl => {
                            self.protocol_focus = false;
                            self.address.select_all();
                        }
                        KeyCode::Tab | KeyCode::BackTab => {
                            self.protocol_focus = !self.protocol_focus
                        }
                        KeyCode::F(2) => self.address.toggle(),
                        KeyCode::Enter if !self.protocol_focus => match self.address.url() {
                            Ok(url) => {
                                self.location(&url);
                                action = Some(Action::Navigate(url));
                                break;
                            }
                            Err(error) => self.status(error),
                        },
                        KeyCode::Enter | KeyCode::Char(' ') if self.protocol_focus => {
                            self.address.toggle()
                        }
                        KeyCode::Left if !self.protocol_focus => self.address.left(),
                        KeyCode::Right if !self.protocol_focus => self.address.right(),
                        KeyCode::Home if !self.protocol_focus => {
                            self.address.cursor = 0;
                            self.address.selected = false;
                        }
                        KeyCode::End if !self.protocol_focus => {
                            self.address.cursor = self.address.text.len();
                            self.address.selected = false;
                        }
                        KeyCode::Backspace if !self.protocol_focus => self.address.backspace(),
                        KeyCode::Delete if !self.protocol_focus => self.address.delete(),
                        KeyCode::Char(c)
                            if !self.protocol_focus
                                && !ctrl
                                && !key.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            self.address.insert(&c.to_string())
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
