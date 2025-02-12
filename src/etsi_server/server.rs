use super::error::EtsiServerError;
use super::state::AppStateEtsi;
use crate::config::{build_hypercube, find_n_shortest_paths, Pqkd};
use crate::util;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::uri::Uri,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum::{body::Bytes, extract::MatchedPath, http::HeaderMap};
use base64::prelude::*;
use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::Span;

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    pub fn keys(self) -> Vec<Key> {
        self.keys
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Prom {
    key_id: String,
    key_id_xor: Option<String>,
    key: Option<Vec<u8>>,
}

impl Prom {
    pub fn new(key_id: String, key_id_xor: Option<String>, key: Option<Vec<u8>>) -> Self {
        Self {
            key_id,
            key_id_xor,
            key,
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key_id_xor(&self) -> &Option<String> {
        &self.key_id_xor
    }

    pub fn key(&self) -> &Option<Vec<u8>> {
        &self.key
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DataKeys {
    from: String,
    to: String,
    path: Vec<String>,
    keys: Vec<Prom>,
}

impl DataKeys {
    pub fn new(from: String, to: String, path: Vec<String>, keys: Vec<Prom>) -> Self {
        Self {
            from,
            to,
            path,
            keys,
        }
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }

    pub fn path(&self) -> &Vec<String> {
        &self.path
    }

    pub fn keys(&self) -> &Vec<Prom> {
        &self.keys
    }
}

pub struct EtsiServer {
    app: Router,
    listener: TcpListener,
}

impl EtsiServer {
    pub async fn build(state: AppStateEtsi, pqkd: &Pqkd) -> Result<EtsiServer, EtsiServerError> {
        let app = Router::new()
            .route("/api/v1/keys/:sae_id/status", get(status))
            .route("/api/v1/keys/:sae_id/enc_keys", get(enc_keys))
            .route("/api/v1/keys/:sae_id/enc_keys", post(enc_keys))
            .route("/api/v1/keys/:sae_id/dec_keys", get(dec_keys))
            .route("/api/v1/keys/:sae_id/dec_keys", post(dec_keys))
            .with_state(state)
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|request: &Request<_>| {
                        // Log the matched route's path (with placeholders not filled in).
                        // Use request.uri() or OriginalUri if you want the real path.
                        let matched_path = request
                            .extensions()
                            .get::<MatchedPath>()
                            .map(MatchedPath::as_str);

                        tracing::info_span!(
                            "http_request",
                            //status_code = tracing::field::Empty,
                            method = ?request.method(),
                            matched_path,
                            status_code = tracing::field::Empty,
                        )
                    })
                    .on_request(|_request: &Request<_>, _span: &Span| {})
                    .on_response(|_response: &Response, _latency: Duration, _span: &Span| {
                        _span.record("status_code", &tracing::field::display(_response.status()));
                    })
                    .on_body_chunk(|_chunk: &Bytes, _latency: Duration, _span: &Span| {})
                    .on_eos(
                        |_trailers: Option<&HeaderMap>,
                         _stream_duration: Duration,
                         _span: &Span| {},
                    )
                    .on_failure(
                        |_error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {},
                    ),
            );
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", pqkd.port())).await?;

        Ok(EtsiServer { app, listener })
    }

    pub async fn run(self) -> Result<(), std::io::Error> {
        axum::serve(self.listener, self.app).await
    }
}

async fn status(
    Path(sae_id): Path<String>,
    State(state): State<AppStateEtsi>,
    req: Request,
) -> Result<Response, StatusCode> {
    tracing::info!("Status with {}", sae_id);
    h(state, req).await.map_err(|e| {
        tracing::error!("{}", e);
        e.into()
    })
}

async fn enc_keys(
    Path(sae_id): Path<String>,
    State(state): State<AppStateEtsi>,
    req: Request,
) -> Result<Response, StatusCode> {
    tracing::info!("To: {}", sae_id);
    _enc_keys(sae_id, state, req).await.map_err(|e| {
        tracing::error!("Transfer keys failed: {}", e);
        e.into()
    })
}

async fn dec_keys(
    Path(sae_id): Path<String>,
    State(state): State<AppStateEtsi>,
    req: Request,
) -> Result<Response, StatusCode> {
    tracing::info!("From: {}", sae_id);
    _dec_keys(sae_id, state, req).await.map_err(|e| {
        tracing::error!("{}", e);
        e.into()
    })
}

async fn _enc_keys(
    sae_id: String,
    state: AppStateEtsi,
    mut req: Request,
) -> Result<Response, EtsiServerError> {
    let pqkd = state
        .pqkd(|p| p.sae_id() == state.sae_id())
        .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?;
    if pqkd.remote_sae_id() == sae_id {
        h(state, req).await
    } else {
        let end = state.hypercube().find_relay(&sae_id).unwrap();
        let hypercube = build_hypercube(state.hypercube().dimension());
        let paths = find_n_shortest_paths(&hypercube, state.id_relay(), end, state.hypercube().n());

        let mut paths_sae_id = Vec::new();

        for path in paths {
            let mut v: Vec<String> = Vec::new();

            let mut p = Vec::new();
            path.iter().for_each(|i| {
                p.push(
                    state
                        .hypercube()
                        .relay()
                        .iter()
                        .find(|r| r.id() == i)
                        .unwrap()
                        .pqkds(),
                );
            });
            let c = state.hypercube().connection();
            for i in 0..p.len() - 1 {
                for sae_id in p[i] {
                    let con = c
                        .iter()
                        .find(|con| con.first() == sae_id || con.second() == sae_id)
                        .unwrap();

                    let s_r = if con.first() == sae_id {
                        con.second()
                    } else {
                        con.first()
                    };

                    let sae_id_r = p[i + 1].iter().find(|s| s == &s_r);

                    if let Some(s) = sae_id_r {
                        v.push(String::from(sae_id));
                        v.push(String::from(s));
                        break;
                    }
                }
            }
            if v.last().unwrap() != &sae_id {
                v.push(sae_id.clone());
            }
            if v.first().unwrap() != state.sae_id() {
                v.insert(0, String::from(state.sae_id()));
            }
            paths_sae_id.push(v);
        }

        let p = paths_sae_id
            .iter()
            .position(|i| i[1] == pqkd.remote_sae_id())
            .unwrap();
        let path = paths_sae_id.swap_remove(p);

        tracing::info!("Main path: {:?}", path);
        tracing::info!("Other path: {:?}", paths_sae_id);

        let uri = match (req.method(), req.uri().query()) {
            (&Method::GET, Some(query)) => {
                format!(
                    "{}/api/v1/keys/{}/enc_keys?{}",
                    pqkd.kme_address(),
                    pqkd.remote_sae_id(),
                    query
                )
            }
            _ => {
                format!(
                    "{}/api/v1/keys/{}/enc_keys",
                    pqkd.kme_address(),
                    pqkd.remote_sae_id()
                )
            }
        };
        *req.uri_mut() = Uri::try_from(uri)?;
        let response = state.client().request(req).await?.into_response();

        if response.status() != StatusCode::OK {
            return Err(EtsiServerError::PqkdRequestError(response.status()));
        }

        let (parts, body) = response.into_parts();
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await?;

        let keys: Keys = serde_json::from_slice(&body_bytes[..])?;
        let keys = keys.keys();

        //let mut list_handles = Vec::new();

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let st = Arc::new(state);
        let ks = Arc::new(keys);
        // let p = paths_sae_id.get(0).unwrap().clone();

        for p in paths_sae_id {
            let tx = tx.clone();
            let st = Arc::clone(&st);
            let ks = Arc::clone(&ks);
            tokio::task::spawn(async move {
                tracing::info!("SEND KEY path {:?}", p);
                let res = send_keys(st, p, ks).await;
                tx.send(res).await.unwrap();
            });
        }
        // tokio::task::spawn(async move {
        //     tracing::info!("SEND KEY path {:?}", p);
        //     let res = send_keys(st, p, ks).await;
        //     tx.send(res).await.unwrap();
        // });

        tokio::task::spawn(async move {
            tracing::info!("SEND KEY path {:?}", path);
            let res = send_keys(st, path, ks).await;
            tx.send(res).await.unwrap();
        });

        //list_handles.push(h);

        //let response_1 = send_keys(&state, path, &keys).await;

        //let mut responses = Vec::new();

        // match send_keys(state, path, keys).await {
        //     Ok(_) => {
        //         tracing::info!("Transfer keys: Succeces");
        //         Ok(Response::from_parts(parts, Body::from(body_bytes)))
        //     } // todo ERROR zwrocic komunikat
        //     Err(e) => Err(e),
        // }

        // if let Err(e) = response_1 {
        //     tracing::info!("Transfer keys: Failed. {:?}", e);
        //     return Err(e);
        // } else {
        //     tracing::info!("Transfer keys: Succeces");
        // }
        //
        // for r in responses {
        //     if let Err(e) = r {
        //         tracing::info!("Transfer keys: Failed. {:?}", e);
        //         return Err(e);
        //     } else {
        //         tracing::info!("Transfer keys: Succeces");
        //     }
        // }
        while let Some(res) = rx.recv().await {
            if let Err(e) = res {
                tracing::error!("Error: {:?}", e);
                return Err(e);
            }
        }

        tracing::info!("Transfer keys: Succeces");
        Ok(Response::from_parts(parts, Body::from(body_bytes)))
    }
}

async fn _dec_keys(
    sae_id: String,
    state: AppStateEtsi,
    req: Request,
) -> Result<Response, EtsiServerError> {
    let pqkd = state
        .pqkd(|p| p.sae_id() == state.sae_id())
        .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?;

    if pqkd.remote_sae_id() == sae_id {
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
                let body = axum::body::to_bytes(req.into_body(), usize::MAX).await?;
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
        tracing::info!("Key IDs: {:?}", key_ids);
        let Ok(keys) = state.get_key(&sae_id, &key_ids) else {
            return Err(EtsiServerError::GetKeysError);
        };
        if !keys.keys.is_empty() {
            Ok(Response::new(Body::from(serde_json::to_string(&keys).unwrap())).into_response())
        } else {
            Ok(Response::new(Body::from("{'message': 'No Key IDs'}")).into_response())
        }
    }
}

async fn h(state: AppStateEtsi, mut req: Request) -> Result<Response, EtsiServerError> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);

    let pqkd = state
        .pqkd(|p| p.sae_id() == state.sae_id())
        .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?;

    let uri = format!("{}{}", pqkd.kme_address(), path_query);

    *req.uri_mut() = Uri::try_from(uri)?;

    Ok(state.client().request(req).await?.into_response())
}

async fn send_keys(
    state: Arc<AppStateEtsi>,
    path: Vec<String>,
    keys: Arc<Vec<Key>>,
) -> Result<(), EtsiServerError> {
    let pqkd = if let Some(pq) = state.pqkd(|p| p.sae_id() == path[1]) {
        pq
    } else {
        state
            .pqkd(|p| p.sae_id() == state.sae_id())
            .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?
    };

    // let pqkd = state
    //     .pqkd(|p| p.sae_id() == state.sae_id())
    //     .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?;
    let position = path
        .iter()
        .position(|i| i == pqkd.sae_id())
        .ok_or(EtsiServerError::PathError)?;

    let next_pqkd = path.get(position + 1).ok_or(EtsiServerError::PathError)?;
    let pqkd = state
        .pqkd(|p| p.remote_sae_id() == next_pqkd)
        .ok_or(EtsiServerError::UnknownPqkd(state.sae_id().to_string()))?;

    tracing::info!("Send keys to next node {}", pqkd.remote_sae_id());

    let data = if position == 0 {
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
            from: String::from(pqkd.sae_id()),
            to: String::from(pqkd.remote_sae_id()),
            path,
            keys: keys_for_send,
        }
    } else {
        let number = keys.len();
        let size = BASE64_STANDARD.decode(keys[0].key.clone()).unwrap().len() * 8;
        let req = hyper::Request::builder()
            .method(hyper::Method::GET)
            .uri(format!(
                "{}/api/v1/keys/{}/enc_keys?size={}&number={}",
                pqkd.kme_address(),
                pqkd.remote_sae_id(),
                size,
                number
            ))
            .body(Body::empty())?;
        let res = state
            .client_for_sae_id(pqkd.sae_id())
            .unwrap()
            .request(req)
            .await?
            .into_response();

        if res.status() != StatusCode::OK {
            return Err(EtsiServerError::PqkdRequestError(res.status()));
        }

        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;

        let keys_for_xor: Keys = serde_json::from_slice(&body_bytes[..])?;

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
            from: String::from(pqkd.sae_id()),
            to: String::from(pqkd.remote_sae_id()),
            path,
            keys: keys_for_send,
        }
    };
    let request = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(format!("{}/info_keys", pqkd.remote_proxy_address()))
        .header("content-type", "application/json")
        .body(Body::new(
            serde_json::to_string(&data).map_err(|_| EtsiServerError::SendKeysError)?,
        ))?;

    let res = state.client().request(request).await?.into_response();

    if res.status() != StatusCode::OK {
        return Err(EtsiServerError::SendKeysError);
    }

    Ok(())
}
