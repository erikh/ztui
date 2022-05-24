use std::{collections::HashMap, time::Instant};

use crossterm::event::{self, Event, KeyCode};
use tui::widgets::{ListItem, ListState};
use zerotier_one_api::types::Network;

#[derive(Debug, Clone)]
pub enum EditingMode {
    Command,
    Editing,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    None,
    Join,
    Config,
}

#[derive(Debug, Clone)]
pub struct App {
    pub editing_mode: EditingMode,
    pub dialog: Dialog,
    pub inputbuffer: String,
    pub listitems: Vec<ListItem<'static>>,
    pub liststate: ListState,
    pub savednetworks: HashMap<String, Network>,
    pub savednetworksidx: Vec<String>,
    pub last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            dialog: Dialog::None,
            editing_mode: EditingMode::Command,
            inputbuffer: String::new(),
            savednetworksidx: Vec::new(),
            savednetworks: HashMap::new(),
            last_usage: HashMap::new(),
            listitems: Vec::new(),
            liststate: ListState::default(),
        }
    }
}

impl App {
    pub fn read_key(&mut self) -> std::io::Result<bool> {
        if let Event::Key(key) = event::read()? {
            match self.editing_mode {
                EditingMode::Command => match key.code {
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
                            let id = self.savednetworksidx[pos].clone();
                            self.savednetworksidx =
                                self.savednetworksidx.splice(pos - 1..pos, []).collect();
                            self.savednetworks.remove(&id);
                        }
                        'l' => {
                            let pos = self.liststate.selected().unwrap_or_default();
                            let id = self.savednetworksidx[pos].clone();
                            tokio::spawn(crate::client::leave_network(id));
                        }
                        'j' => {
                            let pos = self.liststate.selected().unwrap_or_default();
                            let id = self.savednetworksidx[pos].clone();
                            tokio::spawn(crate::client::join_network(id));
                        }
                        'J' => {
                            self.dialog = Dialog::Join;
                            self.editing_mode = EditingMode::Editing;
                        }
                        'c' => {
                            self.inputbuffer =
                                serde_json::to_string_pretty(&self.savednetworks.get(
                                    &self.savednetworksidx
                                        [self.liststate.selected().unwrap_or_default()],
                                ))?;
                            self.dialog = Dialog::Config;
                        }
                        _ => {}
                    },
                    _ => {}
                },
                EditingMode::Editing => match key.code {
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
                },
            }
        }
        Ok(false)
    }
}
