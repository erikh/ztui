use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use http::{HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use zerotier_one_api::types::Network;

struct App {
    listitems: Vec<ListItem<'static>>,
    liststate: ListState,
    savednetworks: HashMap<String, Network>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;
    let mut app = App {
        savednetworks: HashMap::new(),
        listitems: Vec::new(),
        liststate: ListState::default(),
    };

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
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{}", err);
    }

    Ok(())
}

fn draw<B: Backend>(f: &mut Frame<'_, B>, app: &mut App) -> Result<(), anyhow::Error> {
    display_networks(f, app)?;
    display_help(f)?;
    Ok(())
}

// determine the path of the authtoken.secret
fn authtoken_path(arg: Option<&Path>) -> &Path {
    if let Some(arg) = arg {
        return arg;
    }

    if cfg!(target_os = "linux") {
        Path::new("/var/lib/zerotier-one/authtoken.secret")
    } else if cfg!(target_os = "windows") {
        Path::new("C:/ProgramData/ZeroTier/One/authtoken.secret")
    } else if cfg!(target_os = "macos") {
        Path::new("/Library/Application Support/ZeroTier/One/authtoken.secret")
    } else {
        panic!("authtoken.secret not found; please provide the -s option to provide a custom path")
    }
}

fn local_client_from_file(
    authtoken_path: &Path,
) -> Result<zerotier_one_api::Client, anyhow::Error> {
    let authtoken = std::fs::read_to_string(authtoken_path)?;
    local_client(authtoken)
}

fn local_client(authtoken: String) -> Result<zerotier_one_api::Client, anyhow::Error> {
    let mut headers = HeaderMap::new();
    headers.insert("X-ZT1-Auth", HeaderValue::from_str(&authtoken)?);

    Ok(zerotier_one_api::Client::new_with_client(
        "http://127.0.0.1:9993",
        reqwest::Client::builder()
            .default_headers(headers)
            .build()?,
    ))
}

async fn get_networks(s: mpsc::UnboundedSender<Vec<Network>>) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    let networks = client.get_networks().await?;

    s.send(networks.to_vec())?;
    Ok(())
}

fn display_networks<B: Backend>(f: &mut Frame<'_, B>, app: &mut App) -> Result<(), anyhow::Error> {
    let list = Layout::default()
        .constraints([Constraint::Min(4)])
        .split(f.size());

    let titleblock = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title("ZeroTier Terminal UI");

    let (s, mut r) = mpsc::unbounded_channel();

    tokio::spawn(get_networks(s));

    let networks: Vec<Network>;

    'outer: loop {
        match r.try_recv() {
            Ok(n) => {
                networks = n;
                break 'outer;
            }

            Err(_) => std::thread::sleep(Duration::new(0, 10)),
        }
    }

    let mut new = false;

    for network in &networks {
        if !app
            .savednetworks
            .contains_key(&network.subtype_1.id.clone().unwrap())
        {
            new = true;
        }

        app.savednetworks
            .insert(network.subtype_1.id.clone().unwrap(), network.clone());
    }

    app.listitems = app
        .savednetworks
        .iter()
        .map(|(k, v)| {
            ListItem::new(Spans::from(vec![
                Span::styled(k.clone(), Style::default().fg(Color::LightCyan)),
                Span::raw(" "),
                Span::styled(
                    v.subtype_1.name.clone().unwrap(),
                    Style::default().fg(Color::Cyan),
                ),
            ]))
        })
        .collect::<Vec<ListItem>>();

    if new {
        app.liststate = ListState::default();
    }

    let listview = List::new(app.listitems.clone())
        .block(titleblock)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(listview, list[0], &mut app.liststate);
    Ok(())
}

fn display_help<B: Backend>(f: &mut Frame<B>) -> Result<(), anyhow::Error> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::from("A title"));
    let span = Span::from("here's a widget");

    let para = Paragraph::new(span).block(block).wrap(Wrap { trim: true });

    let (w, h) = crossterm::terminal::size()?;
    let mut rect = Rect::default();
    rect.y = h - 5;
    rect.width = w;
    rect.height = h - rect.y;
    f.render_widget(para, rect);
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> std::io::Result<()> {
    loop {
        let last_tick = Instant::now();
        terminal.draw(|f| {
            draw(f, app).unwrap();
        })?;

        let timeout = Duration::new(1, 0)
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
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
                    KeyCode::Char(c) => {
                        if c == 'q' {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
