use std::time::SystemTime;

use time::{Duration, OffsetDateTime};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
    Frame,
};
use zerotier_central_api::types::Member;
use zerotier_one_api::types::Network;

use crate::app::{App, Dialog, ListFilter, Page, STATUS_DISCONNECTED};

macro_rules! filter_disconnected {
    ($app:expr, $val:block) => {
        $app.settings
            .iter()
            .filter_map(|(_, v)| {
                if let ListFilter::Connected = $app.settings.filter() {
                    if v.subtype_1.status.clone().unwrap() != STATUS_DISCONNECTED {
                        Some($val(v))
                    } else {
                        None
                    }
                } else {
                    Some($val(v))
                }
            })
            .collect::<Vec<String>>()
    };
}

macro_rules! get_space_offset {
    ($mapped:expr, $var:expr, $map:block) => {
        " ".repeat(
            1 + get_max_len(
                $mapped
                    .clone()
                    .iter()
                    .filter_map($map)
                    .collect::<Vec<String>>(),
            ) - $var.len(),
        )
    };
}

fn get_max_len(strs: Vec<String>) -> usize {
    strs.iter()
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

fn get_max_savednetworks(app: App) -> usize {
    get_max_len(filter_disconnected!(app, {
        |v: &Network| v.subtype_1.name.clone().unwrap()
    }))
}

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

fn dialog_join<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    dialog(f, app, 10, "Join a Network".to_string())
}

pub fn display_dialogs<B: Backend>(
    f: &mut Frame<'_, B>,
    app: &mut App,
) -> Result<(), anyhow::Error> {
    match app.dialog {
        Dialog::Join => {
            dialog_join(f, app);
        }
        Dialog::APIKey(_) => {
            dialog_api_key(f, app);
        }
        Dialog::Help => {
            dialog_help(f, app.page.clone())?;
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

    let listitems = members
        .iter()
        .map(|m| {
            ListItem::new(Spans::from(vec![
                Span::styled(m.node_id.clone().unwrap(), Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(
                    m.name.clone().unwrap(),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::raw(get_space_offset!(members, m.name.clone().unwrap(), {
                    |m| Some(m.name.clone().unwrap())
                })),
                Span::styled(
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
                ),
            ]))
        })
        .collect::<Vec<ListItem>>();

    let listview = List::new(listitems)
        .block(titleblock)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(listview, list[0], &mut app.liststate);
    Ok(())
}

pub fn display_networks<B: Backend>(
    f: &mut Frame<'_, B>,
    app: &mut App,
    networks: Vec<Network>,
) -> Result<(), anyhow::Error> {
    let list = Layout::default()
        .constraints([Constraint::Min(4)])
        .split(f.size());

    let titleblock = Block::default()
        .borders(Borders::ALL)
        .title("[ ZeroTier Terminal UI | Press h for Help ]");

    let new = app.settings.update_networks(networks)?;

    let listitems = app
        .settings
        .idx_iter()
        .filter_map(|k| {
            let v = app.settings.get(k).unwrap();

            if let ListFilter::Connected = app.settings.filter() {
                if v.subtype_1.status.clone().unwrap() == STATUS_DISCONNECTED {
                    return None;
                }
            }

            Some(ListItem::new(Spans::from(vec![
                Span::styled(k.clone(), Style::default().fg(Color::LightCyan)),
                Span::raw(" "),
                Span::styled(
                    v.subtype_1.name.clone().unwrap_or_default(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" ".repeat(
                    1 + get_max_savednetworks(app.clone())
                        - v.subtype_1.name.clone().unwrap_or_default().len(),
                )),
                Span::styled(
                    v.subtype_1.status.clone().unwrap(),
                    Style::default().fg(match v.subtype_1.status.clone().unwrap().as_str() {
                        "OK" => Color::LightGreen,
                        "REQUESTING_CONFIGURATION" => Color::LightYellow,
                        STATUS_DISCONNECTED => Color::LightRed,
                        _ => Color::LightRed,
                    }),
                ),
                Span::raw(get_space_offset!(
                    app.settings,
                    v.subtype_1.status.clone().unwrap_or_default(),
                    {
                        |(_, v2)| {
                            if let ListFilter::Connected = app.settings.filter() {
                                if v2.subtype_1.status.clone().unwrap() == STATUS_DISCONNECTED {
                                    None
                                } else {
                                    Some(v2.subtype_1.status.clone().unwrap_or_default())
                                }
                            } else {
                                Some(v2.subtype_1.status.clone().unwrap_or_default())
                            }
                        }
                    }
                )),
                Span::styled(
                    v.subtype_1.assigned_addresses.join(", "),
                    Style::default().fg(Color::LightGreen),
                ),
                Span::raw(get_space_offset!(
                    app.settings,
                    v.subtype_1.assigned_addresses.join(", "),
                    { |(_, v2)| Some(v2.subtype_1.assigned_addresses.join(", ")) }
                )),
                Span::styled(
                    if let Some(s) = app
                        .settings
                        .nets
                        .clone()
                        .get_usage(v.subtype_1.port_device_name.clone().unwrap())
                    {
                        s
                    } else {
                        "".to_string()
                    },
                    Style::default().fg(Color::LightMagenta),
                ),
            ])))
        })
        .collect::<Vec<ListItem>>();

    if new {
        app.liststate = ListState::default();
    }

    if app.liststate.selected().is_none() && listitems.len() > 0 {
        app.liststate.select(Some(0));
    }

    let listview = List::new(listitems)
        .block(titleblock)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(listview, list[0], &mut app.liststate);
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
    ],
    vec![
        ["Up/Down", "Navigate the List"],
        ["q", "quit to networks screen"],
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
