use std::path::PathBuf;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{backend::CrosstermBackend, Terminal};

mod app;
mod client;
mod display;
mod nets;

fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .expect("could not locate your home directory")
        .home_dir()
        .join(".networks.zerotier")
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    client::local_client_from_file(client::authtoken_path(None)).expect(
        "must be able to read the authtoken.secret file in the zerotier configuration directory",
    );

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;
    let mut app = app::App::default();

    let networks_file = std::fs::read_to_string(home_dir()).unwrap_or("{}".to_string());
    app.savednetworks = serde_json::from_str(&networks_file)?;

    terminal.clear()?;
    eprintln!("Polling ZeroTier for network information...");

    let res = app.run(&mut terminal);

    std::fs::write(
        home_dir(),
        serde_json::to_string(&app.savednetworks.clone())?,
    )?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{}", err);
    }

    Ok(())
}
