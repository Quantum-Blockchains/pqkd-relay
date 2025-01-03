use crate::config::{Config, Pqkd, Proxy};
use crate::etsi_server::{Key, KeyIds, Keys};
use axum::body::Body;
use hyper_tls::HttpsConnector;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use std::collections::HashMap;
use std::{
    fs,
    sync::{Arc, Mutex},
};

type Client = hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Body>;

pub struct KeyReceived {
    from: String,
    key_id: String,
    key: String,
}

#[derive(Clone)]
pub struct AppStateEtsi {
    sae_id: String,
    pqkds: Vec<Pqkd>,
    keys: Arc<Mutex<Vec<KeyReceived>>>,
    client: Arc<Client>,
}

impl AppStateEtsi {
    pub fn build(
        local_sae_id: &str,
        config: &Config,
        keys: Arc<Mutex<Vec<KeyReceived>>>,
    ) -> AppStateEtsi {
        let pqkd = config
            .pqkds()
            .iter()
            .find(|p| p.sae_id() == local_sae_id)
            .unwrap();
        let client = if let (Some(ca_cert), Some(client_cert), Some(client_key)) =
            (pqkd.ca_cert(), pqkd.client_cert(), pqkd.client_key())
        {
            let ca_cert = fs::read(ca_cert).expect("missing ca cert");
            let client_cert = fs::read(client_cert).expect("missing user cert");
            let client_key = fs::read(client_key).expect("missing user key");

            let identity = native_tls::Identity::from_pkcs8(&client_cert, &client_key)
                .expect("From PEM failed");
            let ca = native_tls::Certificate::from_pem(&ca_cert).expect("From PEM failed");
            let tls_connector = native_tls::TlsConnector::builder()
                .identity(identity)
                .add_root_certificate(ca)
                .build()
                .unwrap();

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

        AppStateEtsi {
            sae_id: String::from(local_sae_id),
            pqkds: config.pqkds().clone(),
            // kme_address: String::from(config.kme_address()),
            // remote_sae_id: String::from(config.remote_sae_id()),
            // remote_proxy_address: String::from(config.remote_proxy_address()),
            // proxies: config.proxies().clone(),
            keys,
            client: Arc::new(client),
        }
    }

    pub fn sae_id(&self) -> &str {
        &self.sae_id
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }

    pub fn client(&self) -> &Arc<Client> {
        &self.client
    }

    pub fn add_key(&self, from: String, key_id: String, key: String) {
        self.keys
            .lock()
            .unwrap()
            .push(KeyReceived { from, key_id, key });
    }

    pub fn get_key(&self, from: &str, key_ids: &KeyIds) -> Keys {
        let mut keys = self.keys.lock().unwrap();
        let mut return_keys = Vec::new();
        key_ids.key_ids.iter().map(|key_id| {
            keys.iter()
                .position(|k| k.from == from && k.key_id == key_id.key_id)
                .map(|i| {
                    let key = keys.swap_remove(i);
                    return_keys.push(Key {
                        key: key.key,
                        key_id: key.key_id,
                    })
                })
        });
        Keys { keys: return_keys }
    }
}

#[derive(Clone)]
pub struct AppStateRelay {
    pqkds: Vec<Pqkd>,
    clients: HashMap<String, Arc<Client>>,
    keys: HashMap<String, Arc<Mutex<Vec<KeyReceived>>>>,
}

impl AppStateRelay {
    pub fn build(
        pqkds: Vec<Pqkd>,
        clients: HashMap<String, Arc<Client>>,
        keys: HashMap<String, Arc<Mutex<Vec<KeyReceived>>>>,
    ) -> AppStateRelay {
        AppStateRelay {
            pqkds,
            clients,
            keys,
        }
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }

    pub fn clients(&self) -> &HashMap<String, Arc<Client>> {
        &self.clients
    }

    pub fn add_key(&self, sae_id: &str, from: String, key_id: String, key: String) {
        self.keys
            .get(sae_id)
            .unwrap()
            .lock()
            .unwrap()
            .push(KeyReceived { from, key_id, key });
    }

    pub fn get_key(&self, sae_id: &str, from: &str, key_ids: &KeyIds) -> Keys {
        let mut keys = self.keys.get(sae_id).unwrap().lock().unwrap();
        let mut return_keys = Vec::new();
        key_ids.key_ids.iter().map(|key_id| {
            keys.iter()
                .position(|k| k.from == from && k.key_id == key_id.key_id)
                .map(|i| {
                    let key = keys.swap_remove(i);
                    return_keys.push(Key {
                        key: key.key,
                        key_id: key.key_id,
                    })
                })
        });
        Keys { keys: return_keys }
    }
}

pub struct AppState {
    local_sae_id: String,
    pqkds: Vec<Pqkd>,
    // kme_address: String,
    // remote_sae_id: String,
    // remote_proxy_address: String,
    // proxies: Vec<Proxy>,
    keys: Mutex<Vec<KeyReceived>>,
    client: Client,
}

impl AppState {
    pub fn build(local_sae_id: &str, config: &Config) -> AppState {
        let pqkd = config
            .pqkds()
            .iter()
            .find(|p| p.sae_id() == local_sae_id)
            .unwrap();
        let client = if let (Some(ca_cert), Some(client_cert), Some(client_key)) =
            (pqkd.ca_cert(), pqkd.client_cert(), pqkd.client_key())
        {
            let ca_cert = fs::read(ca_cert).expect("missing ca cert");
            let client_cert = fs::read(client_cert).expect("missing user cert");
            let client_key = fs::read(client_key).expect("missing user key");

            let identity = native_tls::Identity::from_pkcs8(&client_cert, &client_key)
                .expect("From PEM failed");
            let ca = native_tls::Certificate::from_pem(&ca_cert).expect("From PEM failed");
            let tls_connector = native_tls::TlsConnector::builder()
                .identity(identity)
                .add_root_certificate(ca)
                .build()
                .unwrap();

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

        AppState {
            local_sae_id: String::from(local_sae_id),
            pqkds: config.pqkds().clone(),
            // kme_address: String::from(config.kme_address()),
            // remote_sae_id: String::from(config.remote_sae_id()),
            // remote_proxy_address: String::from(config.remote_proxy_address()),
            // proxies: config.proxies().clone(),
            keys: Mutex::new(Vec::new()),
            client,
        }
    }

    pub fn local_sae_id(&self) -> &str {
        &self.local_sae_id
    }

    pub fn pqkds(&self) -> &Vec<Pqkd> {
        &self.pqkds
    }

    // pub fn kme_address(&self) -> &str {
    //     &self.kme_address
    // }
    //
    // pub fn remote_sae_id(&self) -> &str {
    //     &self.remote_sae_id
    // }
    //
    // pub fn remote_proxy_address(&self) -> &str {
    //     &self.remote_proxy_address
    // }
    //
    // pub fn proxies(&self) -> &Vec<Proxy> {
    //     &self.proxies
    // }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn add_key(&self, from: String, key_id: String, key: String) {
        self.keys
            .lock()
            .unwrap()
            .push(KeyReceived { from, key_id, key });
    }

    pub fn get_key(&self, from: &str, key_ids: &KeyIds) -> Keys {
        let mut keys = self.keys.lock().unwrap();
        let mut return_keys = Vec::new();
        key_ids.key_ids.iter().map(|key_id| {
            keys.iter()
                .position(|k| k.from == from && k.key_id == key_id.key_id)
                .map(|i| {
                    let key = keys.swap_remove(i);
                    return_keys.push(Key {
                        key: key.key,
                        key_id: key.key_id,
                    })
                })
        });
        Keys { keys: return_keys }
    }
}
