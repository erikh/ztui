use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tui::widgets::TableState;

use crate::{
    config::{config_path, Settings},
    terminal::deinit_terminal,
};

mod app;
mod client;
mod config;
mod display;
mod nets;
mod terminal;

fn main() -> Result<(), anyhow::Error> {
    client::local_client_from_file(client::authtoken_path(None)).expect(
        "must be able to read the authtoken.secret file in the zerotier configuration directory",
    );

    let mut terminal = terminal::init_terminal()?;

    let mut app = app::App::default();
    std::fs::create_dir_all(config_path())?;
    let settings = Arc::new(Mutex::new(match Settings::from_dir(config_path()) {
        Ok(c) => c,
        Err(_) => Settings::default(),
    }));

    terminal.clear()?;
    eprintln!("Polling ZeroTier for network information...");

    let s = settings.clone();
    std::thread::spawn(move || start_supervisors(s));
    let res = app.run(&mut terminal, settings.clone());

    settings.lock().unwrap().to_file(config_path())?;
    deinit_terminal(terminal)?;

    res
}

fn start_supervisors(settings: Arc<Mutex<Settings>>) {
    loop {
        let networks = crate::client::sync_get_networks().unwrap();
        let mut lock = settings.lock().unwrap();
        lock.nets.refresh().unwrap();
        if lock.update_networks(networks).unwrap() {
            lock.network_state = TableState::default();
        };
        drop(lock);

        std::thread::sleep(Duration::new(3, 0));
    }
}
