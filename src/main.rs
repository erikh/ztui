use std::{
    collections::{HashMap, HashSet},
    io::BufRead,
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

#[derive(Debug, Clone)]
struct App {
    listitems: Vec<ListItem<'static>>,
    liststate: ListState,
    savednetworks: HashMap<String, Network>,
    savednetworksidx: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;
    let mut app = App {
        savednetworksidx: Vec::new(),
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

fn get_max_len(networks: Vec<String>) -> usize {
    networks
        .iter()
        .max_by(|k, k2| {
            if k.len() > k2.len() {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        })
        .unwrap()
        .len()
}

fn get_max_savednetworks(networks: HashMap<String, Network>) -> usize {
    get_max_len(
        networks
            .iter()
            .map(|(_, v)| v.subtype_1.name.clone().unwrap())
            .collect::<Vec<String>>(),
    )
}

fn display_networks<B: Backend>(f: &mut Frame<'_, B>, app: &mut App) -> Result<(), anyhow::Error> {
    let list = Layout::default()
        .constraints([Constraint::Min(4)])
        .split(f.size());

    let titleblock = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title("[ ZeroTier Terminal UI ]");

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
    let mut ids = HashSet::new();

    for network in &networks {
        let id = network.subtype_1.id.clone().unwrap();

        ids.insert(id.clone());

        if !app.savednetworks.contains_key(&id) {
            new = true;
        }

        app.savednetworks.insert(id, network.clone());
    }

    for (id, network) in app.savednetworks.iter_mut() {
        if !app.savednetworksidx.contains(id) {
            app.savednetworksidx.push(id.clone());
        }

        if !ids.contains(id) {
            network.subtype_1.status = Some("DISCONNECTED".to_string());
        }
    }

    app.listitems = app
        .savednetworksidx
        .iter()
        .map(|k| {
            let v = app.savednetworks.get(k).unwrap();
            ListItem::new(Spans::from(vec![
                Span::styled(k.clone(), Style::default().fg(Color::LightCyan)),
                Span::raw(" "),
                Span::styled(
                    v.subtype_1.name.clone().unwrap_or_default(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" ".repeat(
                    1 + get_max_savednetworks(app.savednetworks.clone())
                        - v.subtype_1.name.clone().unwrap_or_default().len(),
                )),
                Span::styled(
                    v.subtype_1.status.clone().unwrap(),
                    Style::default().fg(match v.subtype_1.status.clone().unwrap().as_str() {
                        "OK" => Color::LightGreen,
                        "REQUESTING_CONFIGURATION" => Color::LightYellow,
                        "DISCONNECTED" => Color::LightRed,
                        _ => Color::LightRed,
                    }),
                ),
                Span::raw(
                    " ".repeat(
                        1 + get_max_len(
                            app.savednetworks
                                .clone()
                                .iter()
                                .map(|(_, v)| v.subtype_1.status.clone().unwrap())
                                .collect::<Vec<String>>(),
                        ) - v.subtype_1.status.clone().unwrap_or_default().len(),
                    ),
                ),
                Span::styled(
                    v.subtype_1.assigned_addresses.join(", "),
                    Style::default().fg(Color::LightGreen),
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
        .title(Span::from("[ Help ]"));

    let help_text = vec![
        "Up/Down = Navigate the List",
        "d = Delete a list member",
        "q = Quit",
        "j = Join a bookmarked network",
        "l = Leave a bookmarked network",
    ];

    let mut spans = Vec::new();

    let ht2 = help_text.clone();
    let mut x = 0;
    loop {
        if help_text.len() < x {
            break;
        }
        let mut three = Vec::new();
        for i in 0..3 {
            if help_text.len() <= i + x {
                break;
            }
            three.push(help_text[i + x]);
        }
        x += 3;
        let mut s = Vec::new();
        let mut y = 0;
        for t in &three {
            y += 1;
            s.push(Span::from(t.to_string()));
            if y < 3 {
                s.push(Span::raw(" ".repeat(
                    1 + get_max_len(ht2.iter().map(|s| s.to_string()).collect::<Vec<String>>())
                        - t.len(),
                )));
            }
        }

        spans.push(Spans::from(s));
    }

    let para = Paragraph::new(spans).block(block).wrap(Wrap { trim: true });

    let (w, h) = crossterm::terminal::size()?;
    let mut rect = Rect::default();
    rect.y = h - 5;
    rect.width = w;
    rect.height = h - rect.y;
    f.render_widget(para, rect);
    Ok(())
}

async fn leave_network(network_id: String) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    Ok(*client.delete_network(&network_id).await?)
}

async fn join_network(network_id: String) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    client
        .update_network(
            &network_id,
            &Network {
                subtype_0: zerotier_one_api::types::NetworkSubtype0 {
                    allow_default: None,
                    allow_dns: None,
                    allow_global: None,
                    allow_managed: None,
                },
                subtype_1: zerotier_one_api::types::NetworkSubtype1 {
                    allow_default: None,
                    allow_dns: None,
                    allow_global: None,
                    allow_managed: None,
                    assigned_addresses: Vec::new(),
                    bridge: None,
                    broadcast_enabled: None,
                    dns: None,
                    id: None,
                    mac: None,
                    mtu: None,
                    multicast_subscriptions: Vec::new(),
                    name: None,
                    netconf_revision: None,
                    port_device_name: None,
                    port_error: None,
                    routes: Vec::new(),
                    status: None,
                    type_: None,
                },
            },
        )
        .await?;
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
                            tokio::spawn(leave_network(id));
                        }
                        'j' => {
                            let pos = app.liststate.selected().unwrap_or_default();
                            let id = app.savednetworksidx[pos].clone();
                            tokio::spawn(join_network(id));
                        }
                        'J' => {
                            let mut network_id = String::new();
                            std::io::stdin().lock().read_line(&mut network_id)?;
                            tokio::spawn(join_network(network_id));
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
}
