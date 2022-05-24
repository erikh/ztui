use std::{collections::HashMap, time::Instant};

use tui::widgets::{ListItem, ListState};
use zerotier_one_api::types::Network;

#[derive(Debug, Clone)]
pub enum EditingMode {
    Command,
    Editing,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    None,
    Join,
    Config,
}

#[derive(Debug, Clone)]
pub struct App {
    pub editing_mode: EditingMode,
    pub dialog: Dialog,
    pub inputbuffer: String,
    pub listitems: Vec<ListItem<'static>>,
    pub liststate: ListState,
    pub savednetworks: HashMap<String, Network>,
    pub savednetworksidx: Vec<String>,
    pub last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            dialog: Dialog::None,
            editing_mode: EditingMode::Command,
            inputbuffer: String::new(),
            savednetworksidx: Vec::new(),
            savednetworks: HashMap::new(),
            last_usage: HashMap::new(),
            listitems: Vec::new(),
            liststate: ListState::default(),
        }
    }
}
