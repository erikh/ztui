use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use tui::widgets::TableState;
use zerotier_one_api::types::Network;

use crate::{app::ListFilter, nets::Nets};

pub fn config_path() -> PathBuf {
    directories::UserDirs::new()
        .expect("could not locate your home directory")
        .home_dir()
        .join(".config.zerotier")
}

fn template(s: Option<&String>, interface: String) -> Option<String> {
    if s.is_none() {
        return None;
    }

    Some(
        s.clone()
            .unwrap()
            .replace("%i", &format!("'{}'", interface)),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    commands: HashMap<char, String>,
}

impl UserConfig {
    pub fn from_dir(filename: PathBuf) -> Result<Self, anyhow::Error> {
        let config_file = std::fs::read_to_string(filename.join("config.json"))?;
        Ok(serde_json::from_str(&config_file)?)
    }

    pub fn command_for(&self, c: char, interface: String) -> Option<String> {
        template(self.commands.get(&c), interface)
    }
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    api_keys: HashMap<String, String>,
    savednetworks: HashMap<String, Network>,
    savednetworksidx: Vec<String>,
    filter: ListFilter,
    #[serde(skip)]
    pub network_state: TableState,
    #[serde(skip)]
    user_config: UserConfig,
    #[serde(skip)]
    pub nets: Nets,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_keys: HashMap::new(),
            user_config: UserConfig::default(),
            network_state: TableState::default(),
            filter: ListFilter::None,
            savednetworks: HashMap::new(),
            savednetworksidx: Vec::new(),
            nets: Nets::new().unwrap(),
        }
    }
}

impl Settings {
    pub fn from_dir(filename: PathBuf) -> Result<Self, anyhow::Error> {
        let config_file = std::fs::read_to_string(filename.join("settings.json"))?;
        let mut config: Self = serde_json::from_str(&config_file)?;

        config.user_config = match UserConfig::from_dir(filename) {
            Ok(uc) => uc,
            Err(_) => UserConfig::default(),
        };

        Ok(config)
    }

    pub fn to_file(&self, filename: PathBuf) -> Result<(), anyhow::Error> {
        Ok(std::fs::write(
            filename.join("settings.json"),
            serde_json::to_string_pretty(self)?,
        )?)
    }

    pub fn user_config(&self) -> UserConfig {
        self.user_config.clone()
    }

    pub fn set_filter(&mut self, filter: ListFilter) {
        self.filter = filter
    }

    pub fn filter(&self) -> ListFilter {
        self.filter.clone()
    }

    pub fn network_count(&self) -> usize {
        self.savednetworksidx.len()
    }

    pub fn update_networks(&mut self, networks: Vec<Network>) -> Result<bool, anyhow::Error> {
        let mut new = false;
        let mut ids = HashSet::new();

        for network in &networks {
            let id = network.subtype_1.id.clone().unwrap();

            ids.insert(id.clone());

            if !self.savednetworks.contains_key(&id) {
                new = true;
            }

            self.savednetworks.insert(id, network.clone());
        }

        for (id, network) in self.savednetworks.iter_mut() {
            if !self.savednetworksidx.contains(id) {
                self.savednetworksidx.push(id.clone());
            }

            if !ids.contains(id) {
                network.subtype_1.status = Some(crate::app::STATUS_DISCONNECTED.to_string());
                continue;
            }

            self.nets
                .store_usage(network.subtype_1.port_device_name.clone().unwrap());
        }

        Ok(new)
    }

    pub fn remove_network(&mut self, pos: usize) {
        let id = self.savednetworksidx[pos].clone();
        self.savednetworksidx = self.savednetworksidx.splice(pos - 1..pos, []).collect();
        self.savednetworks.remove(&id);
    }

    pub fn get_network_by_pos(&self, pos: usize) -> Option<&Network> {
        self.savednetworks.get(&self.get_network_id_by_pos(pos))
    }

    pub fn get_network_id_by_pos(&self, pos: usize) -> String {
        self.savednetworksidx[pos].clone()
    }

    pub fn get(&self, id: &str) -> Option<&Network> {
        self.savednetworks.get(id)
    }

    pub fn idx_iter(&self) -> impl Iterator<Item = &String> {
        self.savednetworksidx.iter()
    }

    pub fn api_key_for_id(&self, id: String) -> Option<&String> {
        self.api_keys.get(&id)
    }

    pub fn set_api_key_for_id(&mut self, id: String, api_key: String) {
        self.api_keys.insert(id, api_key);
    }
}
