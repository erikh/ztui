use std::{collections::HashSet, time::Instant};

use tui::{
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use zerotier_one_api::types::Network;

use crate::app::{App, Dialog, ListFilter, STATUS_DISCONNECTED};

macro_rules! filter_disconnected {
    ($app:expr, $val:block) => {
        $app.savednetworks
            .iter()
            .filter_map(|(_, v)| {
                if let ListFilter::Connected = $app.filter {
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

fn dialog_join<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let w = f.size().width;

    let layout = Layout::default()
        .direction(tui::layout::Direction::Vertical)
        .horizontal_margin(w / 2 - 10)
        .constraints(
            [
                Constraint::Percentage(50),
                Constraint::Length(3),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let orig_len = app.inputbuffer.len();
    let len = layout[1].width as usize - app.inputbuffer.len();

    app.inputbuffer += &" ".repeat(len);
    let p = Paragraph::new(app.inputbuffer.as_ref()).block(
        Block::default()
            .borders(Borders::ALL)
            .title("] Join a Network ["),
    );

    f.render_widget(p, layout[1]);
    app.inputbuffer.truncate(orig_len);
}

pub fn display_dialogs<B: Backend>(
    f: &mut Frame<'_, B>,
    app: &mut App,
) -> Result<(), anyhow::Error> {
    match app.dialog {
        Dialog::Join => dialog_join(f, app),
        _ => {}
    }

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
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title("[ ZeroTier Terminal UI ]");

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
            network.subtype_1.status = Some(STATUS_DISCONNECTED.to_string());
            continue;
        }

        if let Some(net) = app
            .nets
            .find_by_interface(network.subtype_1.port_device_name.clone().unwrap())
        {
            if let Some(v) = app.last_usage.get_mut(&net.interface) {
                v.push((net.rx_bytes as u128, net.tx_bytes as u128, Instant::now()));
                if v.len() > 2 {
                    let v2 = v
                        .iter()
                        .skip(v.len() - 3)
                        .map(|k| *k)
                        .collect::<Vec<(u128, u128, Instant)>>();
                    app.last_usage.insert(net.interface.clone(), v2);
                }
            } else {
                app.last_usage.insert(
                    net.interface.clone(),
                    vec![(net.rx_bytes as u128, net.tx_bytes as u128, Instant::now())],
                );
            }
        }
    }

    app.listitems = app
        .savednetworksidx
        .iter()
        .filter_map(|k| {
            let v = app.savednetworks.get(k).unwrap();

            if let ListFilter::Connected = app.filter {
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
                    app.savednetworks,
                    v.subtype_1.status.clone().unwrap_or_default(),
                    {
                        |(_, v2)| {
                            if let ListFilter::Connected = app.filter {
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
                    app.savednetworks,
                    v.subtype_1.assigned_addresses.join(", "),
                    { |(_, v2)| Some(v2.subtype_1.assigned_addresses.join(", ")) }
                )),
                Span::styled(
                    if let Some(s) = app
                        .clone()
                        .last_usage
                        .get_mut(&v.subtype_1.port_device_name.clone().unwrap())
                    {
                        if s.len() < 2 {
                            "".to_string()
                        } else {
                            let len = s.len();
                            let mut i = s.iter();
                            let first = i.nth(len - 2).unwrap();
                            let mut i = s.iter();
                            let second = i.nth(len - 1).unwrap();

                            // this math is wrong
                            let elapsed =
                                second.2.duration_since(first.2).as_millis() as f64 / 1000 as f64;
                            let rx_bytes = (second.0 as f64 * elapsed) - (first.0 as f64 * elapsed);
                            let tx_bytes = (second.1 as f64 * elapsed) - (first.1 as f64 * elapsed);

                            format!(
                                "Rx: {}/s | Tx: {}/s",
                                byte_unit::Byte::from_bytes(rx_bytes as u128)
                                    .get_appropriate_unit(true)
                                    .to_string(),
                                byte_unit::Byte::from_bytes(tx_bytes as u128)
                                    .get_appropriate_unit(true)
                                    .to_string(),
                            )
                        }
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

    if app.liststate.selected().is_none() && app.listitems.len() > 0 {
        app.liststate.select(Some(0));
    }

    let listview = List::new(app.listitems.clone())
        .block(titleblock)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(listview, list[0], &mut app.liststate);
    Ok(())
}

pub fn display_help<B: Backend>(f: &mut Frame<B>) -> Result<(), anyhow::Error> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::from("[ Help ]"));

    let help_text = vec![
        "Up/Down = Navigate the List",
        "d = Delete a list member",
        "q = Quit",
        "j = Join a bookmarked network",
        "l = Leave a bookmarked network",
        "J = Join a network by address",
        "c = review network config",
        "t = toggle disconnected in list",
    ];

    let mut spans = Vec::new();

    let ht2 = help_text.clone();
    let mut x = 0;
    loop {
        if help_text.len() < x {
            break;
        }
        let mut items = Vec::new();
        for i in 0..help_text.len() / 2 {
            if help_text.len() <= i + x {
                break;
            }
            items.push(help_text[i + x]);
        }
        x += 3;
        let mut s = Vec::new();
        let mut y = 0;
        for t in &items {
            y += 1;
            s.push(Span::from(t.to_string()));
            if y < help_text.len() / 2 {
                s.push(Span::raw(get_space_offset!(ht2, t, {
                    |s| Some(s.to_string())
                })));
            }
        }

        spans.push(Spans::from(s));
    }

    let para = Paragraph::new(spans).block(block).wrap(Wrap { trim: true });

    let size = f.size();
    let w = size.width;
    let h = size.height;
    let mut rect = Rect::default();
    rect.y = h - 5;
    rect.width = w;
    rect.height = h - rect.y;
    f.render_widget(para, rect);
    Ok(())
}
