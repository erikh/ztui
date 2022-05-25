use std::{
    collections::HashMap,
    io::{Read, Write},
    time::{Duration, Instant},
};

use bat::{Input, PrettyPrinter};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
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
                if self.read_key(terminal)? {
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

    pub fn read_key<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
    ) -> std::io::Result<bool> {
        if let Event::Key(key) = event::read()? {
            match self.editing_mode {
                EditingMode::Command => {
                    if self.command_mode_key(terminal, key)? {
                        return Ok(true);
                    }
                }
                EditingMode::Editing => self.edit_mode_key(terminal, key),
            }
        }
        Ok(false)
    }

    fn command_mode_key<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
        key: KeyEvent,
    ) -> std::io::Result<bool> {
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
                x => {
                    if let Some(net) = self
                        .settings
                        .get_network_by_pos(self.liststate.selected().unwrap_or_default())
                    {
                        if let Some(s) = self
                            .settings
                            .user_config()
                            .command_for(x, net.subtype_1.port_device_name.clone().unwrap())
                        {
                            let mut args = vec!["-c"];
                            args.push(&s);

                            crate::temp_mute_terminal!(terminal, {
                                terminal.clear()?;

                                let pty_system = native_pty_system();
                                let pair = pty_system
                                    .openpty(PtySize {
                                        rows: terminal.size().unwrap().height,
                                        cols: terminal.size().unwrap().width,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    })
                                    .unwrap();

                                let mut cmd = CommandBuilder::new("/bin/sh");
                                cmd.args(args);

                                let mut child = pair.slave.spawn_command(cmd).unwrap();
                                let pid = child.process_id().unwrap();

                                let (s, mut r) = mpsc::unbounded_channel();

                                tokio::spawn(async move {
                                    tokio::signal::ctrl_c().await.unwrap();
                                    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                                    s.send(()).unwrap();
                                });

                                let mut reader = pair.master.try_clone_reader().unwrap();
                                let mut writer = pair.master.try_clone_writer().unwrap();

                                std::thread::spawn(move || {
                                    std::io::copy(&mut reader, &mut std::io::stdout().lock())
                                        .unwrap();
                                });

                                std::thread::spawn(move || {
                                    let mut buf = [0u8; 1];

                                    while let Ok(size) = std::io::stdin().lock().read(&mut buf) {
                                        writer.write_all(&buf[0..size]).unwrap();

                                        if let Ok(_) = r.try_recv() {
                                            return;
                                        }
                                    }
                                });

                                child.wait()?;
                            });
                            terminal.clear()?;
                        }
                    }
                }
            },
            _ => {}
        }

        Ok(false)
    }

    fn edit_mode_key<W: Write>(
        &mut self,
        _terminal: &mut Terminal<CrosstermBackend<W>>,
        key: KeyEvent,
    ) {
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
