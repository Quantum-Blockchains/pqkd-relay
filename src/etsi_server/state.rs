use crate::config::{Config, Hypercube, Pqkd};
use crate::etsi_server::{Key, KeyIds, Keys};
use axum::body::Body;
use hyper_tls::HttpsConnector;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use std::{
    fs,
    sync::{Arc, Mutex},
};

use super::error::EtsiServerError;

use std::collections::HashMap;
pub type Client = hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Body>;

pub struct KeyReceived {
    pub num: u8,
    pub from: String,
    pub key_id: String,
    pub key: String,
}

impl KeyReceived {
    pub fn new(from: String, key_id: String, key: String) -> Self {
        Self {
            num: 1u8,
            from,
            key_id,
            key,
        }
    }

    pub fn num(&mut self) {
        self.num += 1;
    }
}

#[derive(Clone)]
pub struct AppStateEtsi {
    id_relay: String,
    sae_id: String,
    pqkds: Vec<Pqkd>,
    keys: Arc<Mutex<Vec<KeyReceived>>>,
    client: Arc<Client>,
    clients: Arc<HashMap<String, Arc<Client>>>,
    hypercube: Arc<Hypercube>,
}

impl AppStateEtsi {
    pub fn build(
        local_sae_id: &str,
        config: &Config,
        keys: Arc<Mutex<Vec<KeyReceived>>>,
        clients: Arc<HashMap<String, Arc<Client>>>,
        hypercube: Arc<Hypercube>,
    ) -> Result<AppStateEtsi, EtsiServerError> {
        let pqkd = config
            .pqkds()
            .iter()
            .find(|p| p.sae_id() == local_sae_id)
            .unwrap();
        let client = if let (Some(ca_cert), Some(client_cert), Some(client_key)) =
            (pqkd.ca_cert(), pqkd.client_cert(), pqkd.client_key())
        {
            let ca_cert = fs::read(ca_cert)?;
            let client_cert = fs::read(client_cert)?;
            let client_key = fs::read(client_key)?;

            let identity = native_tls::Identity::from_pkcs8(&client_cert, &client_key)?;
            let ca = native_tls::Certificate::from_pem(&ca_cert)?;
            let tls_connector = native_tls::TlsConnector::builder()
                .identity(identity)
                .add_root_certificate(ca)
                .build()?;

            let mut http_connectore = HttpConnector::new();
            http_connectore.enforce_http(false);

            let https_connector = HttpsConnector::from((http_connectore, tls_connector.into()));
            let client: Client =
                hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
                    .http1_title_case_headers(true)
                    .build(https_connector);

            client
        } else {
            let https = HttpsConnector::new();
            let client: Client =
                hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
                    .http1_title_case_headers(true)
                    .build(https);

            client
        };

        Ok(AppStateEtsi {
            id_relay: String::from(config.id()),
            sae_id: String::from(local_sae_id),
            pqkds: config.pqkds().clone(),
            keys,
            client: Arc::new(client),
            clients,
            hypercube,
        })
    }

    pub fn id_relay(&self) -> &str {
        &self.id_relay
    }

    pub fn sae_id(&self) -> &str {
        &self.sae_id
    }

    pub fn pqkd<P>(&self, predicate: P) -> Option<&Pqkd>
    where
        P: FnMut(&&Pqkd) -> bool,
    {
        self.pqkds.iter().find(predicate)
    }

    // pub fn pqkd(&self, sae_id: &str) -> Option<&Pqkd> {
    //     println!("1: {}", sae_id);
    //     self.pqkds.iter().find(|p| p.sae_id() == sae_id)
    // }
    //
    // pub fn pqkds(&self) -> &Vec<Pqkd> {
    //     &self.pqkds
    // }

    pub fn client(&self) -> &Arc<Client> {
        &self.client
    }

    pub fn client_for_sae_id(&self, sae_id: &str) -> Option<&Arc<Client>> {
        self.clients.get(sae_id)
    }

    pub fn hypercube(&self) -> &Arc<Hypercube> {
        &self.hypercube
    }

    pub fn get_key(&self, from: &str, key_ids: &KeyIds) -> Result<Keys, EtsiServerError> {
        let mut keys = self
            .keys
            .lock()
            .map_err(|_| EtsiServerError::GetKeysError)?;
        let mut return_keys = Vec::new();
        for key_id in &key_ids.key_ids {
            if let Some(p) = keys
                .iter()
                .position(|k| k.from == from && k.key_id == key_id.key_id && k.num == 2)
            {
                let key = keys.swap_remove(p);
                return_keys.push(Key {
                    key: key.key,
                    key_id: key.key_id,
                });
            };
        }

        Ok(Keys { keys: return_keys })
    }
}

#[cfg(test)]
mod tests {
    use super::{AppStateEtsi, Client, KeyReceived};
    use crate::config::Hypercube;
    use crate::etsi_server::{server::KeyId, KeyIds};
    use hyper_tls::HttpsConnector;
    use hyper_util::rt::TokioExecutor;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn test_hypercube() -> Arc<Hypercube> {
        let toml = r#"
dimension = 2
n = 2

[[relay]]
id = "00"
pqkds = ["Alice"]

[[connection]]
first = "Alice"
second = "Bob"
"#;
        Arc::new(toml::from_str(toml).expect("valid hypercube"))
    }

    fn test_client() -> Arc<Client> {
        let https = HttpsConnector::new();
        let client: Client =
            hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
                .http1_title_case_headers(true)
                .build(https);
        Arc::new(client)
    }

    #[test]
    fn get_key_returns_only_entries_with_num_equal_two() {
        let keys = Arc::new(Mutex::new(vec![
            KeyReceived {
                num: 2,
                from: "Relay_00".to_string(),
                key_id: "k1".to_string(),
                key: "v1".to_string(),
            },
            KeyReceived {
                num: 1,
                from: "Relay_00".to_string(),
                key_id: "k2".to_string(),
                key: "v2".to_string(),
            },
        ]));

        let state = AppStateEtsi {
            id_relay: "00".to_string(),
            sae_id: "Alice".to_string(),
            pqkds: vec![],
            keys: Arc::clone(&keys),
            client: test_client(),
            clients: Arc::new(HashMap::new()),
            hypercube: test_hypercube(),
        };

        let key_ids = KeyIds {
            key_ids: vec![KeyId {
                key_id: "k1".to_string(),
            }],
        };

        let response = state.get_key("Relay_00", &key_ids).expect("get_key ok");
        assert_eq!(response.keys.len(), 1);
        assert_eq!(response.keys[0].key_id, "k1");
        assert_eq!(response.keys[0].key, "v1");

        let store = keys.lock().expect("lock keys");
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].key_id, "k2");
    }

    #[test]
    fn get_key_returns_empty_when_no_matching_items() {
        let state = AppStateEtsi {
            id_relay: "00".to_string(),
            sae_id: "Alice".to_string(),
            pqkds: vec![],
            keys: Arc::new(Mutex::new(Vec::new())),
            client: test_client(),
            clients: Arc::new(HashMap::new()),
            hypercube: test_hypercube(),
        };
        let key_ids = KeyIds {
            key_ids: vec![KeyId {
                key_id: "unknown".to_string(),
            }],
        };

        let response = state.get_key("Relay_00", &key_ids).expect("get_key ok");
        assert!(response.keys.is_empty());
    }
}
