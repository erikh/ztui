use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
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
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Clear, Paragraph, TableState},
    Frame, Terminal,
};

use crate::{client::central_client, config::Settings};

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
    APIKey(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Page {
    Networks,
    Network(String),
}

#[derive(Debug, Clone)]
pub struct App {
    pub editing_mode: EditingMode,
    pub dialog: Dialog,
    pub inputbuffer: String,
    pub last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
    pub page: Page,
    pub member_count: usize,
    pub member_state: TableState,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::Networks,
            dialog: Dialog::None,
            editing_mode: EditingMode::Command,
            inputbuffer: String::new(),
            last_usage: HashMap::new(),
            member_count: 0,
            member_state: TableState::default(),
        }
    }
}

impl App {
    pub fn run<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
        settings: Arc<Mutex<Settings>>,
    ) -> Result<(), anyhow::Error> {
        terminal.clear()?;

        loop {
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
            let s = settings.clone();
            terminal.draw(|f| {
                self.draw(f, s).unwrap();
            })?;

            let timeout = Duration::new(1, 0)
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if crossterm::event::poll(timeout)? {
                if self.read_key(terminal, settings.clone())? {
                    return Ok(());
                }
            }
        }
    }

    fn set_dialog_api_key(&mut self, id: String) {
        self.page = Page::Networks;
        self.dialog = Dialog::APIKey(id);
        self.editing_mode = EditingMode::Editing;
        self.inputbuffer = String::new();
    }

    fn show_error<B: Backend>(&self, f: &mut Frame<'_, B>, mut message: String) {
        let size = f.size();
        message.truncate(size.width as usize - 10);
        let span = Spans::from(vec![Span::styled(
            format!("[ {} ]", message),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )]);

        let rect = Rect::new(
            size.width - span.width() as u16 - 2,
            size.height - 1,
            span.width() as u16,
            1,
        );
        f.render_widget(Clear, rect);
        f.render_widget(Paragraph::new(span), rect);
    }

    fn draw<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        settings: Arc<Mutex<Settings>>,
    ) -> Result<(), anyhow::Error> {
        match self.page.clone() {
            Page::Networks => {
                crate::display::display_networks(f, self, settings.clone())?;
            }
            Page::Network(id) => {
                if let Some(key) = settings.lock().unwrap().api_key_for_id(id.clone()) {
                    let client = central_client(key.to_string())?;
                    match crate::client::sync_get_members(client, id.clone()) {
                        Ok(members) => {
                            crate::display::display_network(f, self, members)?;
                        }
                        Err(e) => {
                            // order is very important here, the recursive draw call must happen
                            // before the error show call otherwise the error doesn't show.
                            // however, if you misorder draw and the set_dialog_api_key call you
                            // will enter an infinite loop.
                            self.set_dialog_api_key(id.clone());
                            self.draw(f, settings.clone())?;
                            self.show_error(
                                f,
                                format!("Invalid API Key for Network ({}): {}", id, e),
                            );
                        }
                    }
                } else {
                    self.set_dialog_api_key(id);
                }
            }
        }

        crate::display::display_dialogs(f, self)?;
        Ok(())
    }

    pub fn read_key<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
        settings: Arc<Mutex<Settings>>,
    ) -> Result<bool, anyhow::Error> {
        if let Event::Key(key) = event::read()? {
            match self.editing_mode {
                EditingMode::Command => {
                    if self.command_mode_key(terminal, settings, key)? {
                        return Ok(true);
                    }
                }
                EditingMode::Editing => self.edit_mode_key(terminal, settings, key),
            }
        }
        Ok(false)
    }

    fn command_mode_key<W: Write>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
        settings: Arc<Mutex<Settings>>,
        key: KeyEvent,
    ) -> Result<bool, anyhow::Error> {
        let mut lock = settings.lock().unwrap();
        match &self.page {
            Page::Network(_) => match key.code {
                KeyCode::Up => {
                    if let Some(pos) = self.member_state.selected() {
                        if pos > 0 {
                            self.member_state.select(Some(pos - 1));
                        }
                    }
                }
                KeyCode::Down => {
                    let pos = self.member_state.selected().unwrap_or_default() + 1;
                    if pos < self.member_count {
                        self.member_state.select(Some(pos))
                    }
                }
                KeyCode::Esc => {
                    self.dialog = Dialog::None;
                    self.editing_mode = EditingMode::Command;
                }
                KeyCode::Char(c) => match c {
                    'q' => {
                        self.page = Page::Networks;
                        self.dialog = Dialog::None;
                        self.editing_mode = EditingMode::Command;
                    }
                    'h' => {
                        self.dialog = match self.dialog {
                            Dialog::Help => Dialog::None,
                            _ => Dialog::Help,
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            Page::Networks => match key.code {
                KeyCode::Up => {
                    if let Some(pos) = lock.network_state.selected() {
                        if pos > 0 {
                            lock.network_state.select(Some(pos - 1));
                        }
                    }
                }
                KeyCode::Down => {
                    let pos = lock.network_state.selected().unwrap_or_default() + 1;
                    if pos < lock.network_count() {
                        lock.network_state.select(Some(pos))
                    }
                }
                KeyCode::Esc => {
                    self.dialog = Dialog::None;
                    self.editing_mode = EditingMode::Command;
                }
                KeyCode::Char(c) => match c {
                    'q' => return Ok(true),
                    'd' => {
                        let pos = lock.network_state.selected().unwrap_or_default();
                        lock.remove_network(pos);
                    }
                    'l' => {
                        let pos = lock.network_state.selected().unwrap_or_default();
                        let id = lock.get_network_id_by_pos(pos);
                        crate::client::leave_network(id)?;
                    }
                    'j' => {
                        let pos = lock.network_state.selected().unwrap_or_default();
                        let id = lock.get_network_id_by_pos(pos);
                        crate::client::join_network(id)?;
                    }
                    'J' => {
                        self.dialog = Dialog::Join;
                        self.editing_mode = EditingMode::Editing;
                        self.inputbuffer = String::new();
                    }
                    'c' => {
                        self.inputbuffer = serde_json::to_string_pretty(&lock.get_network_by_pos(
                            lock.network_state.selected().unwrap_or_default(),
                        ))?;
                        self.dialog = Dialog::Config;
                    }
                    't' => {
                        let filter = match lock.filter() {
                            ListFilter::None => ListFilter::Connected,
                            ListFilter::Connected => ListFilter::None,
                        };

                        lock.set_filter(filter);
                        lock.network_state.select(Some(0));
                    }
                    'h' => {
                        self.dialog = match self.dialog {
                            Dialog::Help => Dialog::None,
                            _ => Dialog::Help,
                        }
                    }
                    's' => {
                        let lock = lock;

                        let id = lock.get_network_id_by_pos(
                            lock.network_state.selected().unwrap_or_default(),
                        );
                        let key = lock.api_key_for_id(id.clone());
                        if let Some(_) = key {
                            self.member_state.select(Some(0));
                            self.page = Page::Network(id)
                        } else {
                            self.dialog = Dialog::APIKey(id);
                            self.editing_mode = EditingMode::Editing;
                            self.inputbuffer = String::new();
                        }
                    }
                    x => {
                        if let Some(net) = lock
                            .get_network_by_pos(lock.network_state.selected().unwrap_or_default())
                        {
                            if let Some(s) = settings
                                .lock()
                                .unwrap()
                                .user_config()
                                .command_for(x, net.subtype_1.port_device_name.clone().unwrap())
                            {
                                App::run_command(terminal, s)?;
                            }
                        }
                    }
                },
                _ => {}
            },
        }

        Ok(false)
    }

    fn edit_mode_key<W: Write>(
        &mut self,
        _terminal: &mut Terminal<CrosstermBackend<W>>,
        settings: Arc<Mutex<Settings>>,
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
                match &self.dialog {
                    Dialog::Join => {
                        crate::client::join_network(self.inputbuffer.clone()).unwrap();
                    }
                    Dialog::APIKey(id) => {
                        settings
                            .lock()
                            .unwrap()
                            .set_api_key_for_id(id.clone(), self.inputbuffer.clone());
                        self.page = Page::Network(id.clone());
                    }
                    _ => {}
                }

                self.inputbuffer = String::new();
                self.dialog = Dialog::None;
                self.editing_mode = EditingMode::Command;
            }
            _ => {}
        }
    }

    fn run_command<W: Write>(
        terminal: &mut Terminal<CrosstermBackend<W>>,
        s: String,
    ) -> Result<(), anyhow::Error> {
        let mut args = vec!["-c"];
        args.push(&s);

        crate::temp_mute_terminal!(terminal, {
            terminal.clear()?;

            let pty_system = native_pty_system();
            let pair = pty_system.openpty(PtySize {
                rows: terminal.size().unwrap().height,
                cols: terminal.size().unwrap().width,
                pixel_width: 0,
                pixel_height: 0,
            })?;

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
                std::io::copy(&mut reader, &mut std::io::stdout().lock()).unwrap();
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

            eprintln!("\nPress ENTER to continue");
            let mut buf = [0u8; 1];
            std::io::stdin().lock().read(&mut buf).unwrap();
        });

        terminal.clear()?;
        Ok(())
    }
}
