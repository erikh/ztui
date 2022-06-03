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
use signal::Signal::{SIGCHLD, SIGINT, SIGTERM};
use tokio::sync::mpsc;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Clear, Paragraph, TableState},
    Frame, Terminal,
};

use crate::{
    client::{self, central_client},
    config::Settings,
};

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
    RenameMember(String, String),
    AddMember(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Page {
    Networks,
    Network(String),
}

impl Default for Page {
    fn default() -> Self {
        Page::Networks
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub editing_mode: EditingMode,
    pub dialog: Dialog,
    pub inputbuffer: String,
    pub last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
    pub member_count: usize,
    pub member_state: TableState,
}

impl Default for App {
    fn default() -> Self {
        Self {
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

    fn set_dialog_api_key(&mut self, settings: Arc<Mutex<Settings>>, id: String) {
        settings.lock().unwrap().page = Page::Networks;
        self.dialog = Dialog::APIKey(id);
        self.editing_mode = EditingMode::Editing;
        self.inputbuffer = String::new();
    }

    fn show_toast<B: Backend>(&self, f: &mut Frame<'_, B>, color: Color, mut message: String) {
        let size = f.size();
        message.truncate(size.width as usize - 10);
        let span = Spans::from(vec![Span::styled(
            format!("[ {} ]", message),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
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
        let lock = settings.lock().unwrap();
        let page = lock.page.clone();
        drop(lock);

        match page {
            Page::Networks => {
                crate::display::display_networks(f, self, settings.clone())?;
            }
            Page::Network(id) => {
                let lock = settings.lock().unwrap();
                let members = lock.members.clone();
                let members = members.get(&id);
                let err = lock.last_error.clone();
                drop(lock);

                if let Some(err) = err {
                    self.show_toast(f, Color::LightRed, err);
                    self.set_dialog_api_key(settings.clone(), id);
                }

                if let Some(members) = members {
                    crate::display::display_network(f, self, members.to_vec())?;
                } else {
                    self.show_toast(
                        f,
                        Color::LightGreen,
                        "Loading your results, please wait...".to_string(),
                    )
                }
            }
        }

        crate::display::display_dialogs(f, self, settings)?;
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
        match &lock.page {
            Page::Network(id) => match key.code {
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
                        lock.page = Page::Networks;
                        self.member_state.select(Some(0));
                        self.dialog = Dialog::None;
                        self.editing_mode = EditingMode::Command;
                    }
                    'h' => {
                        self.dialog = match self.dialog {
                            Dialog::Help => Dialog::None,
                            _ => Dialog::Help,
                        }
                    }
                    'r' => {
                        if let Some(members) = &lock.members.get(id) {
                            if let Some(selected) = self.member_state.selected() {
                                self.dialog = Dialog::RenameMember(
                                    members[selected].network_id.clone().unwrap(),
                                    members[selected].node_id.clone().unwrap(),
                                );
                                self.editing_mode = EditingMode::Editing;
                                self.inputbuffer = members[selected].name.clone().unwrap();
                            }
                        }
                    }
                    'A' => {
                        self.dialog = Dialog::AddMember(id.to_string());
                        self.editing_mode = EditingMode::Editing;
                        self.inputbuffer = String::new();
                    }
                    'a' => {
                        if let Some(members) = &lock.members.get(id) {
                            if let Some(selected) = self.member_state.selected() {
                                let node_id = members[selected].node_id.clone().unwrap();
                                let client = central_client(
                                    lock.api_key_for_id(id.to_string()).unwrap().to_string(),
                                )?;
                                crate::client::sync_authorize_member(
                                    client,
                                    id.to_string(),
                                    node_id,
                                )?;
                            }
                        }
                    }
                    'd' => {
                        if let Some(members) = &lock.members.get(id) {
                            if let Some(selected) = self.member_state.selected() {
                                let node_id = members[selected].node_id.clone().unwrap();
                                let client = central_client(
                                    lock.api_key_for_id(id.to_string()).unwrap().to_string(),
                                )?;
                                crate::client::sync_deauthorize_member(
                                    client,
                                    id.to_string(),
                                    node_id,
                                )?;
                            }
                        }
                    }
                    'D' => {
                        if let Some(members) = &lock.members.get(id) {
                            if let Some(selected) = self.member_state.selected() {
                                let node_id = members[selected].node_id.clone().unwrap();
                                let client = central_client(
                                    lock.api_key_for_id(id.to_string()).unwrap().to_string(),
                                )?;
                                crate::client::sync_delete_member(client, id.to_string(), node_id)?;
                            }
                        }
                    }
                    x => {
                        if let Some(members) = &lock.members.get(id) {
                            {
                                if let Some(member) = members
                                    .iter()
                                    .nth(lock.network_state.selected().unwrap_or_default())
                                {
                                    if let Some(s) =
                                        lock.user_config().command_for_member(x, member)
                                    {
                                        App::run_command(terminal, s)?;
                                    }
                                }
                            }
                        }
                    }
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
                    let count = lock
                        .idx_iter()
                        .filter(|x| {
                            if let ListFilter::Connected = lock.filter() {
                                lock.get(&x).unwrap().subtype_1.status.clone().unwrap()
                                    != STATUS_DISCONNECTED
                            } else {
                                true
                            }
                        })
                        .count();
                    if pos < count {
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
                        let id = lock.get_network_id_by_pos(
                            lock.network_state.selected().unwrap_or_default(),
                        );
                        let key = lock.api_key_for_id(id.clone());
                        if let Some(_) = key {
                            self.member_state.select(Some(0));
                            lock.page = Page::Network(id)
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
                            if let Some(s) = lock.user_config().command_for_network(x, net) {
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
                        let mut lock = settings.lock().unwrap();
                        lock.set_api_key_for_id(id.clone(), self.inputbuffer.clone());
                        lock.page = Page::Network(id.clone());
                    }
                    Dialog::AddMember(network_id) => {
                        let lock = settings.lock().unwrap();
                        crate::client::sync_authorize_member(
                            central_client(
                                lock.api_key_for_id(network_id.to_string())
                                    .unwrap()
                                    .to_string(),
                            )
                            .unwrap(),
                            network_id.to_string(),
                            self.inputbuffer.clone(),
                        )
                        .unwrap();
                    }
                    Dialog::RenameMember(network_id, member_id) => {
                        let mut lock = settings.lock().unwrap();
                        client::sync_update_member_name(
                            central_client(
                                lock.api_key_for_id(network_id.to_string())
                                    .unwrap()
                                    .to_string(),
                            )
                            .unwrap(),
                            network_id.to_string(),
                            member_id.to_string(),
                            self.inputbuffer.clone(),
                        )
                        .unwrap();
                        lock.page = Page::Network(network_id.clone());
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

            let (s, mut r) = mpsc::unbounded_channel();

            let mut child = pair.slave.spawn_command(cmd).unwrap();
            let pid = child.process_id().unwrap();

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

            let trap = signal::trap::Trap::trap(&[SIGINT, SIGTERM, SIGCHLD]);

            for sig in trap {
                match sig {
                    SIGCHLD => {
                        child.wait()?;
                    }
                    _ => {
                        nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(pid as i32),
                            Some(nix::sys::signal::SIGTERM),
                        )
                        .unwrap();
                    }
                }
                break;
            }
            s.send(()).unwrap();

            eprintln!("\nPress ENTER to continue");
            let mut buf = [0u8; 1];
            std::io::stdin().lock().read(&mut buf).unwrap();
        });

        terminal.clear()?;
        Ok(())
    }
}
