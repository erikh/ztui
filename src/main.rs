use crate::terminal::deinit_terminal;

mod app;
mod client;
mod config;
mod display;
mod nets;
mod terminal;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    client::local_client_from_file(client::authtoken_path(None)).expect(
        "must be able to read the authtoken.secret file in the zerotier configuration directory",
    );

    let mut terminal = terminal::init_terminal()?;
    let mut app = app::App::default();

    let networks_file = std::fs::read_to_string(config::config_path()).unwrap_or("{}".to_string());
    app.savednetworks = serde_json::from_str(&networks_file)?;

    terminal.clear()?;
    eprintln!("Polling ZeroTier for network information...");

    let res = app.run(&mut terminal);

    std::fs::write(
        config::config_path(),
        serde_json::to_string(&app.savednetworks.clone())?,
    )?;

    deinit_terminal(terminal)?;

    if let Err(err) = res {
        println!("{}", err);
    }

    Ok(())
}
