use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use bat::{Input, PrettyPrinter};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    Frame, Terminal,
};

mod app;
mod client;
mod display;

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
    let res = run_app(&mut terminal, &mut app);

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

fn draw<B: Backend>(f: &mut Frame<'_, B>, app: &mut app::App) -> Result<(), anyhow::Error> {
    display::display_networks(f, app)?;
    display::display_help(f)?;
    display::display_dialogs(f, app)?;

    Ok(())
}

fn run_app<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    app: &mut app::App,
) -> std::io::Result<()> {
    loop {
        if let app::Dialog::Config = app.dialog {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            terminal.show_cursor()?;
            PrettyPrinter::new()
                .input(Input::from_bytes(app.inputbuffer.as_bytes()).name("config.json"))
                .paging_mode(bat::PagingMode::Always)
                .print()
                .expect("could not print");

            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.hide_cursor()?;
            terminal.clear()?;
            app.dialog = app::Dialog::None;
        }

        let last_tick = Instant::now();
        terminal.draw(|f| {
            draw(f, app).unwrap();
        })?;

        let timeout = Duration::new(1, 0)
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if app.read_key()? {
                return Ok(());
            }
        }
    }
}
