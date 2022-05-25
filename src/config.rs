use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use zerotier_one_api::types::Network;

use crate::nets::Nets;

pub fn config_path() -> PathBuf {
    directories::UserDirs::new()
        .expect("could not locate your home directory")
        .home_dir()
        .join(".config.zerotier")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    savednetworks: HashMap<String, Network>,
    savednetworksidx: Vec<String>,
    #[serde(skip)]
    pub nets: Nets,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            savednetworks: HashMap::new(),
            savednetworksidx: Vec::new(),
            nets: Nets::new().unwrap(),
        }
    }
}

impl Config {
    pub fn from_file(filename: PathBuf) -> Result<Self, anyhow::Error> {
        let config_file = std::fs::read_to_string(filename).unwrap_or("{}".to_string());
        Ok(serde_json::from_str(&config_file)?)
    }

    pub fn to_file(&self, filename: PathBuf) -> Result<(), anyhow::Error> {
        Ok(std::fs::write(filename, serde_json::to_string(self)?)?)
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
                .store_usage(network.subtype_1.port_device_name.clone().unwrap())?;
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

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Network)> {
        self.savednetworks.iter()
    }

    pub fn idx_iter(&self) -> impl Iterator<Item = &String> {
        self.savednetworksidx.iter()
    }
}
