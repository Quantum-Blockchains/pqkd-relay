use crate::config::Config;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::uri::Uri,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::app_state::AppState;
use crate::util;
use hyper::{Method, StatusCode};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
pub struct Key {
    pub key: String,
    #[serde(rename(deserialize = "key_ID"))]
    pub key_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Keys {
    pub keys: Vec<Key>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeyId {
    #[serde(rename(deserialize = "key_ID"))]
    pub key_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeyIds {
    #[serde(rename(deserialize = "key_IDs"))]
    pub key_ids: Vec<KeyId>,
}

#[derive(Deserialize)]
struct DecKeysQuery {
    #[serde(rename(deserialize = "key_ID"))]
    key_id: String,
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

pub struct EtsiServer {
    app: Router,
    listener: TcpListener,
}

impl EtsiServer {
    pub async fn build(state: Arc<AppState>, config: &Config) -> EtsiServer {
        let app = Router::new()
            .route("/api/v1/keys/:sae_id/status", get(status))
            .route("/api/v1/keys/:sae_id/enc_keys", get(enc_keys))
            .route("/api/v1/keys/:sae_id/enc_keys", post(enc_keys))
            .route("/api/v1/keys/:sae_id/dec_keys", get(dec_keys))
            .route("/api/v1/keys/:sae_id/dec_keys", post(dec_keys))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.port_etsi()))
            .await
            .unwrap();

        EtsiServer { app, listener }
    }

    pub async fn run(self) -> Result<(), std::io::Error> {
        axum::serve(self.listener, self.app).await
    }
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

async fn dec_keys(
    Path(sae_id): Path<String>,
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Result<Response, StatusCode> {
    if state.remote_sae_id() == &sae_id {
        h(state, req).await
    } else {
        let key_ids: KeyIds = match *req.method() {
            Method::GET => {
                if let Some(param) = req.uri().query() {
                    let query: Result<DecKeysQuery, _> = serde_qs::from_str(param);
                    if query.is_err() {
                        return Ok(
                            Response::new(Body::from("{'message': 'No Key IDs'}")).into_response()
                        );
                    }
                    let keyid = KeyId {
                        key_id: query.unwrap().key_id,
                    };
                    KeyIds {
                        key_ids: vec![keyid],
                    }
                } else {
                    return Ok(
                        Response::new(Body::from("{'message': 'No Key IDs'}")).into_response()
                    );
                }
            }
            Method::POST => {
                let body = axum::body::to_bytes(req.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let keys_ids: Result<KeyIds, _> = serde_json::from_slice(&body[..]);
                if keys_ids.is_err() {
                    return Ok(
                        Response::new(Body::from("{'message': 'No Key IDs'}")).into_response()
                    );
                }
                keys_ids.unwrap()
            }
            _ => KeyIds { key_ids: vec![] },
        };
        let keys = state.get_key(&sae_id, &key_ids);
        if keys.keys.len() > 0 {
            Ok(Response::new(Body::from(serde_json::to_string(&keys).unwrap())).into_response())
        } else {
            Ok(Response::new(Body::empty()).into_response())
        }
    }
}

async fn enc_keys(
    Path(sae_id): Path<String>,
    State(state): State<Arc<AppState>>,
    mut req: Request,
) -> Result<Response, StatusCode> {
    if state.remote_sae_id() == &sae_id {
        println!("REQUEST WITHOUT INTERMEDIARIES");
        h(state, req).await
    } else {
        // todo
        println!("REQUEST WITH INTERMEDIARIES");
        let path = Vec::from([
            String::from("Test_1SAE"),
            String::from("Test_2SAE"),
            String::from("Validator_1SAE"),
            String::from("Validator_2SAE"),
        ]);

        let uri = match (req.method(), req.uri().query()) {
            (&Method::GET, Some(query)) => {
                format!(
                    "{}/api/v1/keys/{}/enc_keys?{}",
                    state.kme_address(),
                    state.remote_sae_id(),
                    query
                )
            }
            _ => {
                format!(
                    "{}/api/v1/keys/{}/enc_keys",
                    state.kme_address(),
                    state.remote_sae_id()
                )
            }
        };
        *req.uri_mut() = Uri::try_from(uri).unwrap();
        let response = state.client().request(req).await.unwrap().into_response();

        if response.status() != StatusCode::OK {
            // todo ERROR
            return Ok(response);
        }

        let (parts, body) = response.into_parts();
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();

        let keys: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
        let keys = keys.keys();

        match send_keys(state, path, keys).await {
            Ok(_) => Ok(Response::from_parts(parts, Body::from(body_bytes))),
            // todo ERROR
            Err(_) => Ok(Response::new(Body::empty()).into_response()),
        }
    }
}

async fn h(state: Arc<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);
    let uri = format!("{}{}", state.kme_address(), path_query);

    *req.uri_mut() = Uri::try_from(uri).unwrap();

    Ok(state
        .client()
        .request(req)
        .await
        .map_err(|e| {
            println!("ERROR: {:?}", e);
            StatusCode::BAD_REQUEST
        })?
        .into_response())
}

async fn status(
    Path(_sae_id): Path<String>,
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Result<Response, StatusCode> {
    h(state, req).await
}
