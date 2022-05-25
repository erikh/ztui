use std::{
    collections::HashMap,
    io::Write,
    time::{Duration, Instant},
};

use bat::{Input, PrettyPrinter};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};
use tui::{
    backend::{Backend, CrosstermBackend},
    widgets::{ListItem, ListState},
    Frame, Terminal,
};
use zerotier_one_api::types::Network;

use crate::config::Settings;

pub const STATUS_DISCONNECTED: &str = "DISCONNECTED";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditingMode {
    Command,
    Editing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListFilter {
    None,
    Connected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dialog {
    None,
    Join,
    Config,
    Help,
}

#[derive(Debug, Clone)]
pub struct App {
    pub editing_mode: EditingMode,
    pub settings: Settings,
    pub dialog: Dialog,
    pub inputbuffer: String,
    pub listitems: Vec<ListItem<'static>>,
    pub liststate: ListState,
    pub last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            dialog: Dialog::None,
            editing_mode: EditingMode::Command,
            inputbuffer: String::new(),
            last_usage: HashMap::new(),
            listitems: Vec::new(),
            liststate: ListState::default(),
        }
    }
}

impl App {
    pub fn run<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
    ) -> Result<(), anyhow::Error> {
        terminal.clear()?;
        loop {
            let networks = crate::client::sync_get_networks()?;
            self.settings.nets.refresh()?;

            if let Dialog::Config = self.dialog {
                crate::temp_mute_terminal!(terminal, {
                    PrettyPrinter::new()
                        .input(Input::from_bytes(self.inputbuffer.as_bytes()).name("settings.json"))
                        .paging_mode(bat::PagingMode::Always)
                        .print()
                        .expect("could not print");
                });
                self.dialog = Dialog::None;
                self.inputbuffer = String::new();
            }

            let last_tick = Instant::now();
            terminal.draw(|f| {
                self.draw(f, networks).unwrap();
            })?;

            let timeout = Duration::new(1, 0)
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if crossterm::event::poll(timeout)? {
                if self.read_key()? {
                    return Ok(());
                }
            }
        }
    }

    fn draw<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        networks: Vec<Network>,
    ) -> Result<(), anyhow::Error> {
        crate::display::display_networks(f, self, networks)?;
        crate::display::display_dialogs(f, self)?;

        Ok(())
    }

    pub fn read_key(&mut self) -> std::io::Result<bool> {
        if let Event::Key(key) = event::read()? {
            match self.editing_mode {
                EditingMode::Command => {
                    if self.command_mode_key(key)? {
                        return Ok(true);
                    }
                }
                EditingMode::Editing => self.edit_mode_key(key),
            }
        }
        Ok(false)
    }

    fn command_mode_key(&mut self, key: KeyEvent) -> std::io::Result<bool> {
        match key.code {
            KeyCode::Up => {
                if let Some(pos) = self.liststate.selected() {
                    if pos > 0 {
                        self.liststate.select(Some(pos - 1));
                    }
                }
            }
            KeyCode::Down => {
                let pos = self.liststate.selected().unwrap_or_default() + 1;
                if pos < self.listitems.len() {
                    self.liststate.select(Some(pos))
                }
            }
            KeyCode::Esc => {
                self.dialog = Dialog::None;
                self.editing_mode = EditingMode::Command;
            }
            KeyCode::Char(c) => match c {
                'q' => return Ok(true),
                'd' => {
                    let pos = self.liststate.selected().unwrap_or_default();
                    self.settings.remove_network(pos);
                }
                'l' => {
                    let pos = self.liststate.selected().unwrap_or_default();
                    let id = self.settings.get_network_id_by_pos(pos);
                    tokio::spawn(crate::client::leave_network(id));
                }
                'j' => {
                    let pos = self.liststate.selected().unwrap_or_default();
                    let id = self.settings.get_network_id_by_pos(pos);
                    tokio::spawn(crate::client::join_network(id));
                }
                'J' => {
                    self.dialog = Dialog::Join;
                    self.editing_mode = EditingMode::Editing;
                    self.inputbuffer = String::new();
                }
                'c' => {
                    self.inputbuffer = serde_json::to_string_pretty(
                        &self
                            .settings
                            .get_network_by_pos(self.liststate.selected().unwrap_or_default()),
                    )?;
                    self.dialog = Dialog::Config;
                }
                't' => self.settings.set_filter(match self.settings.filter() {
                    ListFilter::None => ListFilter::Connected,
                    ListFilter::Connected => ListFilter::None,
                }),
                'h' => {
                    self.dialog = match self.dialog {
                        Dialog::Help => Dialog::None,
                        _ => Dialog::Help,
                    }
                }
                _ => {}
            },
            _ => {}
        }

        Ok(false)
    }

    fn edit_mode_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(x) => {
                self.inputbuffer.push(x);
            }
            KeyCode::Esc => {
                self.inputbuffer = String::new();
                self.dialog = Dialog::None;
                self.editing_mode = EditingMode::Command;
            }
            KeyCode::Backspace => {
                if self.inputbuffer.len() > 0 {
                    self.inputbuffer
                        .drain(self.inputbuffer.len() - 1..self.inputbuffer.len());
                }
            }
            KeyCode::Enter => {
                tokio::spawn(crate::client::join_network(self.inputbuffer.clone()));
                self.inputbuffer = String::new();
                self.dialog = Dialog::None;
                self.editing_mode = EditingMode::Command;
            }
            _ => {}
        }
    }
}
