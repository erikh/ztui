use std::{collections::HashMap, time::Instant};

use anyhow::anyhow;
use sys_metrics::network::IoNet;

#[derive(Clone, Debug)]
pub struct Nets {
    nets: Vec<IoNet>,
    last_usage: HashMap<String, Vec<(u128, u128, Instant)>>,
}

impl Nets {
    pub fn new() -> Result<Self, anyhow::Error> {
        Ok(Self {
            last_usage: HashMap::new(),
            nets: sys_metrics::network::get_ionets()?,
        })
    }

    pub fn refresh(&mut self) -> Result<(), anyhow::Error> {
        self.nets = sys_metrics::network::get_ionets()?;
        Ok(())
    }

    pub fn find_by_interface(&self, interface: String) -> Option<IoNet> {
        for net in &self.nets {
            if interface == net.interface {
                return Some(net.clone());
            }
        }

        None
    }

    pub fn store_usage(&mut self, interface: String) -> Result<(), anyhow::Error> {
        if let Some(net) = self.find_by_interface(interface.clone()) {
            if let Some(v) = self.last_usage.get_mut(&interface) {
                v.push((net.rx_bytes as u128, net.tx_bytes as u128, Instant::now()));
                if v.len() > 2 {
                    let v2 = v
                        .iter()
                        .skip(v.len() - 3)
                        .map(|k| *k)
                        .collect::<Vec<(u128, u128, Instant)>>();
                    self.last_usage.insert(net.interface.clone(), v2);
                }
            } else {
                self.last_usage.insert(
                    net.interface.clone(),
                    vec![(net.rx_bytes as u128, net.tx_bytes as u128, Instant::now())],
                );
            }

            Ok(())
        } else {
            Err(anyhow!("invalid interface '{}'", interface))
        }
    }

    pub fn get_usage(&mut self, interface: String) -> Option<String> {
        if let Some(s) = self.last_usage.get_mut(&interface) {
            if s.len() < 2 {
                return None;
            } else {
                let len = s.len();
                let mut i = s.iter();
                let first = i.nth(len - 2).unwrap();
                let mut i = s.iter();
                let second = i.nth(len - 1).unwrap();

                // this math is wrong
                let elapsed = second.2.duration_since(first.2).as_millis() as f64 / 1000 as f64;
                let rx_bytes = (second.0 as f64 * elapsed) - (first.0 as f64 * elapsed);
                let tx_bytes = (second.1 as f64 * elapsed) - (first.1 as f64 * elapsed);

                Some(format!(
                    "Rx: {}/s | Tx: {}/s",
                    byte_unit::Byte::from_bytes(rx_bytes as u128)
                        .get_appropriate_unit(true)
                        .to_string(),
                    byte_unit::Byte::from_bytes(tx_bytes as u128)
                        .get_appropriate_unit(true)
                        .to_string(),
                ))
            }
        } else {
            None
        }
    }
}