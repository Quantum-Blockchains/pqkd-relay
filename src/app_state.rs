use crate::config::{Config, Proxy};
use crate::etsi_server::{Key, KeyIds, Keys};
use axum::body::Body;
use hyper_tls::HttpsConnector;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use std::{fs, sync::Mutex};

type Client = hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Body>;

pub struct KeyReceived {
    from: String,
    key_id: String,
    key: String,
}

pub struct AppState {
    local_sae_id: String,
    kme_address: String,
    remote_sae_id: String,
    remote_proxy_address: String,
    proxies: Vec<Proxy>,
    keys: Mutex<Vec<KeyReceived>>,
    client: Client,
}

impl AppState {
    pub fn build(config: &Config) -> AppState {
        let client = if let (Some(ca_cert), Some(client_cert), Some(client_key)) =
            (config.ca_cert(), config.client_cert(), config.client_key())
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
            local_sae_id: String::from(config.local_sae_id()),
            kme_address: String::from(config.kme_address()),
            remote_sae_id: String::from(config.remote_sae_id()),
            remote_proxy_address: String::from(config.remote_proxy_address()),
            proxies: config.proxies().clone(),
            keys: Mutex::new(Vec::new()),
            client,
        }
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

    pub fn proxies(&self) -> &Vec<Proxy> {
        &self.proxies
    }

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
