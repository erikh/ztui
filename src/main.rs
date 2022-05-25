use crate::{
    config::{config_path, Config},
    terminal::deinit_terminal,
};

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
    app.config = match Config::from_file(config_path()) {
        Ok(c) => c,
        Err(_) => Config::default(),
    };

    terminal.clear()?;
    eprintln!("Polling ZeroTier for network information...");

    let res = app.run(&mut terminal);

    app.config.to_file(config_path())?;
    deinit_terminal(terminal)?;

    if let Err(err) = res {
        println!("{}", err);
    }

    Ok(())
}
