use sys_metrics::network::IoNet;

#[derive(Clone, Debug)]
pub struct Nets {
    nets: Vec<IoNet>,
}

impl Nets {
    pub fn new() -> Result<Self, anyhow::Error> {
        Ok(Self {
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
}
