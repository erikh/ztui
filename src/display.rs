use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
};

use time::{Duration, OffsetDateTime};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};
use zerotier_central_api::types::Member;

use crate::{
    app::{App, Dialog, ListFilter, Page, STATUS_DISCONNECTED},
    config::Settings,
};

fn dialog<B: Backend>(f: &mut Frame<B>, app: &mut App, margin: u16, help_text: String) {
    let w = f.size().width;

    let layout = Layout::default()
        .direction(tui::layout::Direction::Vertical)
        .horizontal_margin(w / 2 - margin)
        .constraints(
            [
                Constraint::Percentage(50),
                Constraint::Length(3),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let p = Paragraph::new(app.inputbuffer.as_ref()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("] {} [", help_text)),
    );

    f.render_widget(Clear, layout[1]);
    f.render_widget(p, layout[1]);
}

fn dialog_api_key<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    dialog(f, app, 20, "Enter your Network API Key".to_string())
}

fn dialog_rename_member<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    dialog(f, app, 20, "Enter the new name".to_string())
}

fn dialog_add_member<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    dialog(f, app, 20, "Enter the new node ID".to_string())
}

fn dialog_join<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    dialog(f, app, 10, "Join a Network".to_string())
}

pub fn display_dialogs<B: Backend>(
    f: &mut Frame<'_, B>,
    app: &mut App,
    settings: Arc<Mutex<Settings>>,
) -> Result<(), anyhow::Error> {
    match app.dialog {
        Dialog::Join => {
            dialog_join(f, app);
        }
        Dialog::APIKey(_) => {
            dialog_api_key(f, app);
        }
        Dialog::Help => {
            dialog_help(f, settings.lock().unwrap().page.clone())?;
        }
        Dialog::RenameMember(_, _) => {
            dialog_rename_member(f, app);
        }
        Dialog::AddMember(_) => {
            dialog_add_member(f, app);
        }
        _ => {}
    }

    Ok(())
}

pub fn display_network<B: Backend>(
    f: &mut Frame<'_, B>,
    app: &mut App,
    members: Vec<Member>,
) -> Result<(), anyhow::Error> {
    let list = Layout::default()
        .constraints([Constraint::Min(4)])
        .split(f.size());

    let titleblock = Block::default()
        .borders(Borders::ALL)
        .title("[ ZeroTier Terminal UI | Press h for Help ]");

    let rows = members
        .iter()
        .map(|m| {
            let authed = m.config.clone().unwrap().authorized.unwrap_or_default();
            let caps = m.config.clone().unwrap().capabilities.unwrap();

            Row::new(vec![
                Cell::from(Span::styled(
                    m.node_id.clone().unwrap(),
                    Style::default().fg(Color::Cyan),
                )),
                Cell::from(Span::styled(
                    m.name.clone().unwrap(),
                    Style::default().fg(Color::LightCyan),
                )),
                Cell::from(Span::styled(
                    format!(
                        "{}",
                        fancy_duration::FancyDuration::new(
                            OffsetDateTime::from(SystemTime::now())
                                - OffsetDateTime::UNIX_EPOCH
                                    .checked_add(Duration::new(m.last_online.unwrap() / 1000, 0))
                                    .unwrap()
                        )
                        .to_string()
                    ),
                    Style::default().fg(Color::LightCyan),
                )),
                Cell::from(Span::styled(
                    m.config
                        .clone()
                        .unwrap()
                        .ip_assignments
                        .unwrap_or_default()
                        .join(", "),
                    Style::default().fg(Color::LightGreen),
                )),
                Cell::from(Span::styled(
                    if authed { "Auth" } else { "Unauth" },
                    Style::default().fg(if authed {
                        Color::LightGreen
                    } else {
                        Color::LightRed
                    }),
                )),
                Cell::from(Span::styled(
                    caps.iter()
                        .map(|x| format!("{}", x))
                        .collect::<Vec<String>>()
                        .join(", "),
                    Style::default().fg(Color::LightGreen),
                )),
            ])
        })
        .collect::<Vec<Row>>();

    app.member_count = rows.len();

    let table = Table::new(rows)
        .block(titleblock)
        .widths(&[
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Length(25),
            Constraint::Length(25),
            Constraint::Length(8),
            Constraint::Length(15),
        ])
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(table, list[0], &mut app.member_state);
    Ok(())
}

pub fn display_networks<B: Backend>(
    f: &mut Frame<'_, B>,
    _app: &mut App,
    settings: Arc<Mutex<Settings>>,
) -> Result<(), anyhow::Error> {
    let list = Layout::default()
        .constraints([Constraint::Min(4)])
        .split(f.size());

    let titleblock = Block::default()
        .borders(Borders::ALL)
        .title("[ ZeroTier Terminal UI | Press h for Help ]");

    let mut lock = settings.lock().unwrap();

    let rows = lock
        .idx_iter()
        .filter_map(|k| {
            let v = lock.get(k).unwrap();

            if let ListFilter::Connected = lock.filter() {
                if v.subtype_1.status.clone().unwrap() == STATUS_DISCONNECTED {
                    return None;
                }
            }

            Some(Row::new(vec![
                Cell::from(Span::styled(
                    k.clone(),
                    Style::default().fg(Color::LightCyan),
                )),
                Cell::from(Span::styled(
                    v.subtype_1.name.clone().unwrap_or_default(),
                    Style::default().fg(Color::Cyan),
                )),
                Cell::from(Span::styled(
                    v.subtype_1.status.clone().unwrap(),
                    Style::default().fg(match v.subtype_1.status.clone().unwrap().as_str() {
                        "OK" => Color::LightGreen,
                        "REQUESTING_CONFIGURATION" => Color::LightYellow,
                        STATUS_DISCONNECTED => Color::LightRed,
                        _ => Color::LightRed,
                    }),
                )),
                Cell::from(Span::styled(
                    v.subtype_1.assigned_addresses.join(", "),
                    Style::default().fg(Color::LightGreen),
                )),
                Cell::from(Span::styled(
                    if let Some(s) = lock
                        .nets
                        .clone()
                        .get_usage(v.subtype_1.port_device_name.clone().unwrap())
                    {
                        s
                    } else {
                        "".to_string()
                    },
                    Style::default().fg(Color::LightMagenta),
                )),
            ]))
        })
        .collect::<Vec<Row>>();

    if lock.network_state.selected().is_none() && rows.len() > 0 {
        lock.network_state.select(Some(0));
    }

    let table = Table::new(rows)
        .block(titleblock)
        .widths(&[
            Constraint::Length(16),
            Constraint::Length(20),
            Constraint::Length(15),
            Constraint::Length(20),
            Constraint::Length(35),
        ])
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(table, list[0], &mut lock.network_state);
    Ok(())
}

lazy_static::lazy_static! {
static ref HELP_TEXT: Vec<Vec<[&'static str; 2]>> = vec![
    vec![
        ["Up/Down", "Navigate the List"],
        ["<Esc>", "back out of something"],
        ["d", "Delete a list member"],
        ["q", "Quit"],
        ["j", "Join a bookmarked network"],
        ["l", "Leave a bookmarked network"],
        ["J", "Join a network by address"],
        ["c", "review network settings"],
        ["t", "toggle disconnected in list"],
        ["s", "show network members (requires API key)"],
    ],
    vec![
        ["Up/Down", "Navigate the List"],
        ["q", "quit to networks screen"],
        ["r", "Rename a Member"],
        ["a", "Authorize a deauthorized member"],
        ["A", "Authorize an arbitrary member ID"],
        ["d", "Deauthorize an authorized member"],
        ["D", "Delete a member"],
    ],
];
}

pub fn dialog_help<B: Backend>(f: &mut Frame<B>, page: Page) -> Result<(), anyhow::Error> {
    let size = f.size();
    let w = size.width;
    let h = size.height;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::from("[ Help ]"));

    let help_text = &HELP_TEXT[match page {
        Page::Networks => 0,
        Page::Network(_) => 1,
    }];

    let rows = help_text
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s[0].to_string()),
                Cell::from(s[1].to_string()),
            ])
        })
        .collect::<Vec<Row>>();

    let table = Table::new(rows)
        .block(block)
        .widths(&[Constraint::Length(10), Constraint::Percentage(100)]);

    let mut rect = Rect::default();
    rect.x = w / 4;
    rect.y = h / 4;
    rect.width = w / 2;
    rect.height = h / 2;
    f.render_widget(Clear, rect);
    f.render_widget(table, rect);
    Ok(())
}
