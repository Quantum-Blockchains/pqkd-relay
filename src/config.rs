use serde::{Deserialize, Serialize};
use std::{error, fs, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Proxy {
    sae_id: String,
    addr: String,
}

impl Proxy {
    pub fn sae_id(&self) -> &str {
        &self.sae_id
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    port: u16,
    port_etsi: u16,
    local_sae_id: String,
    remote_sae_id: String,
    remote_proxy_address: String,
    kme_address: String,
    proxies: Vec<Proxy>,
    //targets_sae_id: Vec<String>,
    ca_cert: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
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

    pub fn port_etsi(&self) -> u16 {
        self.port_etsi
    }

    pub fn local_sae_id(&self) -> &str {
        &self.local_sae_id
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
    //pub fn targets_sae_id(&self) -> Vec<String> {
    //    self.targets_sae_id.clone()
    //}

    pub fn ca_cert(&self) -> &Option<PathBuf> {
        &self.ca_cert
    }

    pub fn client_cert(&self) -> &Option<PathBuf> {
        &self.client_cert
    }

    pub fn client_key(&self) -> &Option<PathBuf> {
        &self.client_key
    }

    pub fn proxies(&self) -> &Vec<Proxy> {
        &self.proxies
    }
}
