use crate::config::Config;
use crate::util;
use axum::{
    body::Body,
    extract::{Json, State},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::app_state::AppState;

#[derive(Serialize, Deserialize, Debug)]
struct Key {
    key: String,
    #[serde(rename(deserialize = "key_ID"))]
    key_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Keys {
    keys: Vec<Key>,
}

impl Keys {
    fn keys(self) -> Vec<Key> {
        self.keys
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Prom {
    key_id: String,
    key_id_xor: Option<String>,
    key: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DataKeys {
    path: Vec<String>,
    keys: Vec<Prom>,
}

pub struct RelayServer {
    app: Router,
    listener: TcpListener,
}

impl RelayServer {
    pub async fn build(state: Arc<AppState>, config: &Config) -> RelayServer {
        let app = Router::new()
            .route("/keys", post(request_keys))
            .route("/info_keys", post(info_keys))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.port()))
            .await
            .unwrap();

        RelayServer { app, listener }
    }

    pub async fn run(self) -> Result<(), std::io::Error> {
        axum::serve(self.listener, self.app).await
    }
}

async fn request_keys(
    State(_state): State<Arc<AppState>>,
    Json(_payload): Json<DataKeys>,
) -> Result<Response, StatusCode> {
    Ok(Response::new(Body::empty()).into_response())
}

async fn send_keys(
    state: Arc<AppState>,
    path: Vec<String>,
    keys: Vec<Key>,
) -> Result<(), StatusCode> {
    println!("SEND KEYS");
    let position = path.iter().position(|i| i == state.local_sae_id()).unwrap();

    let next_pqkd = path.get(position + 1).unwrap();

    if next_pqkd == state.remote_sae_id() {
        println!("WITH PQKD");
        let data = if position == 0 {
            println!("SEND FROM 0");
            let keys_ids: Vec<String> = keys.iter().map(|k| k.key_id.clone()).collect();
            let keys_for_send: Vec<Prom> = keys_ids
                .iter()
                .map(|k| Prom {
                    key_id: String::from(k),
                    key_id_xor: None,
                    key: None,
                })
                .collect();
            DataKeys {
                path,
                keys: keys_for_send,
            }
        } else {
            println!("SEND jak posrednik");
            let number = keys.len();
            let size = keys[0].key.len();

            let req = hyper::Request::builder()
                .method(hyper::Method::GET)
                .uri(format!(
                    "{}/api/v1/keys/{}/enc_keys?size={}&number={}",
                    state.kme_address(),
                    state.remote_sae_id(),
                    512,
                    number
                ))
                .body(Body::empty())
                .unwrap();
            println!("req: {:?}", req);
            let res = state.client().request(req).await.unwrap().into_response();

            let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();

            let keys_for_xor: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
            let keys_for_xor = keys_for_xor.keys();

            let mut keys_for_send = Vec::new();

            for i in 0..keys.len() {
                keys_for_send.push(Prom {
                    key_id: keys[i].key_id.clone(),
                    key_id_xor: Some(keys_for_xor[i].key_id.clone()),
                    key: Some(util::xor(
                        keys[i].key.as_bytes().to_vec(),
                        keys_for_xor[i].key.as_bytes().to_vec(),
                    )),
                });
            }

            DataKeys {
                path,
                keys: keys_for_send,
            }
        };
        let request = hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(format!("{}/info_keys", state.remote_proxy_address()))
            .header("content-type", "application/json")
            .body(Body::new(serde_json::to_string(&data).unwrap()))
            .unwrap();

        let response = state
            .client()
            .request(request)
            .await
            .unwrap()
            .into_response();

        println!(
            "body: {:?}",
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
        );
        return Ok(());
    }

    println!("len: {:?}", state.proxies().len());
    println!("proxies: {:?}", state.proxies());
    state
        .proxies()
        .iter()
        .for_each(|k| println!("{:?}", k.sae_id()));

    if let Some(p) = state.proxies().iter().find(|&p| p.sae_id() == next_pqkd) {
        println!("SEND poprostu do prosy");
        let key_for_send: Vec<Prom> = keys
            .iter()
            .map(|k| Prom {
                key_id: k.key_id.clone(),
                key_id_xor: None,
                key: Some(k.key.as_bytes().to_vec()),
            })
            .collect();

        let data = DataKeys {
            path,
            keys: key_for_send,
        };

        let request = hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(format!("{}/info_keys", p.addr()))
            .header("content-type", "application/json")
            .body(Body::new(serde_json::to_string(&data).unwrap()))
            .unwrap();

        let response = state
            .client()
            .request(request)
            .await
            .unwrap()
            .into_response();

        println!(
            "body: {:?}",
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
        );
        return Ok(());
    }

    Ok(())
}

async fn info_keys(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DataKeys>,
) -> Result<Response, StatusCode> {
    println!("PAYLOAD: {:?}", payload);

    let keys = if let Ok(keys) = get_keys(Arc::clone(&state), payload.keys).await {
        keys
    } else {
        return Ok(Response::new(Body::empty()).into_response());
    };

    println!("KEYS: {:?}", keys);

    let position = payload
        .path
        .iter()
        .position(|i| i == state.local_sae_id())
        .unwrap();

    if payload.path.last().unwrap() == state.local_sae_id() {
        for key in keys {
            state.add_key(payload.path[0].to_string(), key.key_id, key.key);
        }

        Ok(Response::new(Body::empty()).into_response())
    } else {
        let next_pqkd = payload.path.get(position + 1).unwrap();

        println!("\tFROM: {:?}", payload.path[0]);
        println!("\tTO: {:?}", payload.path.last().unwrap());
        println!("\tFROM me: {:?}", state.remote_sae_id());
        println!("\tME: {:?}", state.local_sae_id());
        println!("\tNEXT: {:?}", next_pqkd);
        println!("\tKEYS: {:?}", keys);

        match send_keys(state, payload.path, keys).await {
            Ok(_) => Ok(Response::new(Body::empty())),
            // todo ERROR
            Err(_) => Ok(Response::new(Body::empty()).into_response()),
        }
    }
}

async fn get_keys(state: Arc<AppState>, payload_keys: Vec<Prom>) -> Result<Vec<Key>, StatusCode> {
    let mut keys: Vec<Key> = Vec::new();

    for key in payload_keys {
        match (key.key_id, key.key_id_xor, key.key) {
            // jesli proxy przekazuje kluczy proxy obok
            (k_id, None, Some(k)) => {
                println!("dlugosc: {:?}", k.len());
                keys.push(Key {
                    key_id: k_id,
                    key: String::from_utf8(k).unwrap(),
                });
            }
            // jesli wysyla pierwszy wezel
            (k_id, None, None) => {
                let request = hyper::Request::builder()
                    .method(hyper::Method::GET)
                    .uri(format!(
                        "{}/api/v1/keys/{}/dec_keys?key_ID={}",
                        state.kme_address(),
                        state.remote_sae_id(),
                        k_id,
                    ))
                    .body(Body::empty())
                    .unwrap();

                let response = state
                    .client()
                    .request(request)
                    .await
                    .unwrap()
                    .into_response();
                let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let keys_from_pqkd: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
                let keys_from_pqkd = keys_from_pqkd.keys();
                let key_from_pqkd = keys_from_pqkd.get(0).unwrap();
                keys.push(Key {
                    key: key_from_pqkd.key.to_string(),
                    key_id: key_from_pqkd.key_id.to_string(),
                });
            }
            // jesli wezel posredni wysyla kluczy nastepnemu pqkd
            (k_id, Some(k_id_xor), Some(k)) => {
                let request = hyper::Request::builder()
                    .method(hyper::Method::GET)
                    .uri(format!(
                        "{}/api/v1/keys/{}/dec_keys?key_ID={}",
                        state.kme_address(),
                        state.remote_sae_id(),
                        k_id_xor,
                    ))
                    .body(Body::empty())
                    .unwrap();

                let response = state
                    .client()
                    .request(request)
                    .await
                    .unwrap()
                    .into_response();
                let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let keys_from_pqkd: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
                let keys_from_pqkd = keys_from_pqkd.keys();
                let key_from_pqkd = keys_from_pqkd.get(0).unwrap();
                let key_before_xor = util::xor(k, key_from_pqkd.key.as_bytes().to_vec());
                let key_to_string = String::from_utf8(key_before_xor).unwrap();
                let k = Key {
                    key: key_to_string,
                    key_id: k_id,
                };
                keys.push(k);
            }
            _ => {}
        };
    }
    Ok(keys)
}
