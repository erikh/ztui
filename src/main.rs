use std::{
    io::Write,
    time::{Duration, Instant},
};

use bat::{Input, PrettyPrinter};
use crossterm::{
    event::{self, Event, KeyCode},
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

    let networks_file = std::fs::read_to_string(
        directories::UserDirs::new()
            .expect("could not locate your home directory")
            .home_dir()
            .join(".networks.zerotier"),
    )
    .unwrap_or("{}".to_string());
    app.savednetworks = serde_json::from_str(&networks_file)?;

    terminal.clear()?;
    let res = run_app(&mut terminal, &mut app);

    std::fs::write(
        directories::UserDirs::new()
            .expect("could not locate your home directory")
            .home_dir()
            .join(".networks.zerotier"),
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
            if let Event::Key(key) = event::read()? {
                match app.editing_mode {
                    app::EditingMode::Command => match key.code {
                        KeyCode::Up => {
                            if let Some(pos) = app.liststate.selected() {
                                if pos > 0 {
                                    app.liststate.select(Some(pos - 1));
                                }
                            }
                        }
                        KeyCode::Down => {
                            let pos = app.liststate.selected().unwrap_or_default() + 1;
                            if pos < app.listitems.len() {
                                app.liststate.select(Some(pos))
                            }
                        }
                        KeyCode::Esc => {
                            app.dialog = app::Dialog::None;
                            app.editing_mode = app::EditingMode::Command;
                        }
                        KeyCode::Char(c) => match c {
                            'q' => {
                                return Ok(());
                            }
                            'd' => {
                                let pos = app.liststate.selected().unwrap_or_default();
                                let id = app.savednetworksidx[pos].clone();
                                app.savednetworksidx =
                                    app.savednetworksidx.splice(pos - 1..pos, []).collect();
                                app.savednetworks.remove(&id);
                            }
                            'l' => {
                                let pos = app.liststate.selected().unwrap_or_default();
                                let id = app.savednetworksidx[pos].clone();
                                tokio::spawn(client::leave_network(id));
                            }
                            'j' => {
                                let pos = app.liststate.selected().unwrap_or_default();
                                let id = app.savednetworksidx[pos].clone();
                                tokio::spawn(client::join_network(id));
                            }
                            'J' => {
                                app.dialog = app::Dialog::Join;
                                app.editing_mode = app::EditingMode::Editing;
                            }
                            'c' => {
                                app.inputbuffer =
                                    serde_json::to_string_pretty(&app.savednetworks.get(
                                        &app.savednetworksidx
                                            [app.liststate.selected().unwrap_or_default()],
                                    ))?;
                                app.dialog = app::Dialog::Config;
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    app::EditingMode::Editing => match key.code {
                        KeyCode::Char(x) => {
                            app.inputbuffer.push(x);
                        }
                        KeyCode::Esc => {
                            app.inputbuffer = String::new();
                            app.dialog = app::Dialog::None;
                            app.editing_mode = app::EditingMode::Command;
                        }
                        KeyCode::Backspace => {
                            if app.inputbuffer.len() > 0 {
                                app.inputbuffer
                                    .drain(app.inputbuffer.len() - 1..app.inputbuffer.len());
                            }
                        }
                        KeyCode::Enter => {
                            tokio::spawn(client::join_network(app.inputbuffer.clone()));
                            app.inputbuffer = String::new();
                            app.dialog = app::Dialog::None;
                            app.editing_mode = app::EditingMode::Command;
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}
