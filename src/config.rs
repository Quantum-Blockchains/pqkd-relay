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
    id: String,
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

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Relay {
    id: String,
    pqkds: Vec<String>,
}

impl Relay {
    #[cfg(test)]
    pub fn new(id: String, pqkds: Vec<String>) -> Self {
        Relay { id, pqkds }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pqkds(&self) -> &Vec<String> {
        &self.pqkds
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Connection {
    first: String,
    second: String,
    first_sae: String,
    second_sae: String,
}

impl Connection {
    #[cfg(test)]
    pub fn new(first: String, second: String, first_sae: String, second_sae: String) -> Self {
        Connection {
            first,
            second,
            first_sae,
            second_sae,
        }
    }

    pub fn first(&self) -> &str {
        &self.first
    }

    pub fn second(&self) -> &str {
        &self.second
    }

    pub fn first_sae(&self) -> &str {
        &self.first_sae
    }

    pub fn second_sae(&self) -> &str {
        &self.second_sae
    }
}

#[cfg(test)]
mod tests {}
