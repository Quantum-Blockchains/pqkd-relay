use serde::{Deserialize, Serialize};
use std::{error, fs, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pqkd {
    port: u16,
    sae_id: String,
    remote_sae_id: String,
    remote_proxy_address: String,
    kme_address: String,
    ca_cert: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
}

impl Pqkd {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn sae_id(&self) -> &str {
        &self.sae_id
    }

    pub fn kme_address(&self) -> &str {
        &self.kme_address
    }

    pub fn remote_sae_id(&self) -> &str {
        &self.remote_sae_id
    }

    pub fn remote_proxy_address(&self) -> &str {
        &self.remote_proxy_address
    }

    pub fn ca_cert(&self) -> &Option<PathBuf> {
        &self.ca_cert
    }

    pub fn client_cert(&self) -> &Option<PathBuf> {
        &self.client_cert
    }

    pub fn client_key(&self) -> &Option<PathBuf> {
        &self.client_key
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    port: u16,
    pqkds: Vec<Pqkd>,
}

impl Config {
    pub fn build(config_path: PathBuf) -> Result<Config, Box<dyn error::Error>> {
        let data = fs::read(config_path)?;
        let text = String::from_utf8(data)?;
        let config: Config = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }
}
