use super::state::AppStateRelay;
use crate::config::Config;
use crate::etsi_server::{DataKeys, Key, Keys, Prom};
use crate::util;
use axum::{
    body::Body,
    extract::{Json, State},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use hyper::StatusCode;
use tokio::net::TcpListener;

use base64::prelude::*;
use std::time::Duration;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{info_span, Span};

use axum::{
    body::Bytes,
    extract::MatchedPath,
    http::{HeaderMap, Request},
};

pub struct RelayServer {
    app: Router,
    listener: TcpListener,
}

impl RelayServer {
    pub async fn build(state: AppStateRelay, config: &Config) -> RelayServer {
        let app = Router::new()
            //.route("/keys", post(request_keys))
            .route("/info_keys", post(info_keys))
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

                        info_span!(
                            "http_request",
                            method = ?request.method(),
                            matched_path,
                            some_other_field = tracing::field::Empty,
                        )
                    })
                    .on_request(|_request: &Request<_>, _span: &Span| {
                        // You can use `_span.record("some_other_field", value)` in one of these
                        // closures to attach a value to the initially empty field in the info_span
                        // created above.
                    })
                    .on_response(|_response: &Response, _latency: Duration, _span: &Span| {
                        // ...
                    })
                    .on_body_chunk(|_chunk: &Bytes, _latency: Duration, _span: &Span| {
                        // ...
                    })
                    .on_eos(
                        |_trailers: Option<&HeaderMap>,
                         _stream_duration: Duration,
                         _span: &Span| {
                            // ...
                        },
                    )
                    .on_failure(
                        |_error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                            // ...
                        },
                    ),
            );

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port()))
            .await
            .unwrap();

        RelayServer { app, listener }
    }

    pub async fn run(self) -> Result<(), std::io::Error> {
        axum::serve(self.listener, self.app).await
    }
}

async fn info_keys(
    State(state): State<AppStateRelay>,
    Json(payload): Json<DataKeys>,
) -> Result<Response, StatusCode> {
    tracing::info!(
        "Received keys from {} for {}",
        payload.from(),
        payload.path().last().unwrap()
    );
    let keys = if let Ok(keys) = get_keys(payload.to(), &state, payload.keys()).await {
        keys
    } else {
        return Ok(Response::new(Body::empty()).into_response());
    };

    let pqkd = state.pqkd(|p| p.sae_id() == payload.to()).unwrap();

    if payload.path().last().unwrap() == pqkd.sae_id() {
        for key in keys {
            tracing::info!(
                "Save key from {:?} with key_ID: {:?}",
                payload.path()[0],
                key.key_id
            );

            state
                .add_key(
                    pqkd.sae_id(),
                    payload.path()[0].to_string(),
                    key.key_id,
                    key.key,
                )
                .unwrap();
        }
        Ok(Response::new(Body::empty()).into_response())
    } else {
        match send_keys(&state, pqkd.sae_id(), payload.path(), keys).await {
            Ok(_) => Ok(Response::new(Body::empty())),
            // todo ERROR
            Err(_) => Ok(Response::new(Body::empty()).into_response()),
        }
    }
}

async fn send_keys(
    state: &AppStateRelay,
    sae_id: &str,
    path: &[String],
    keys: Vec<Key>,
) -> Result<(), StatusCode> {
    let position = path.iter().position(|i| i == sae_id).unwrap();
    let next_pqkd = path.get(position + 1).unwrap();

    let pqkd = state.pqkd(|p| p.sae_id() == next_pqkd).unwrap();

    if position + 1 == path.len() - 1 {
        for key in keys {
            tracing::info!("Save key from {:?} with key_ID: {:?}", path[0], key.key_id);

            state
                .add_key(pqkd.sae_id(), path[0].to_string(), key.key_id, key.key)
                .unwrap();
        }
        return Ok(());
    }

    tracing::info!("Send keys to next node {}", pqkd.remote_sae_id());

    let client = state.client(pqkd.sae_id()).unwrap();

    let data = if position == 0 {
        let keys_ids: Vec<String> = keys.iter().map(|k| k.key_id.clone()).collect();
        let keys_for_send: Vec<Prom> = keys_ids
            .iter()
            .map(|k| Prom::new(String::from(k), None, None))
            .collect();
        DataKeys::new(
            String::from(pqkd.sae_id()),
            String::from(pqkd.remote_sae_id()),
            Vec::from(path),
            keys_for_send,
        )
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
            .body(Body::empty())
            .unwrap();

        let res = client.request(req).await.unwrap().into_response();

        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();

        let keys_for_xor: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
        let keys_for_xor = keys_for_xor.keys();

        let mut keys_for_send = Vec::new();

        for i in 0..keys.len() {
            keys_for_send.push(Prom::new(
                keys[i].key_id.clone(),
                Some(keys_for_xor[i].key_id.clone()),
                Some(util::xor(
                    keys[i].key.as_bytes().to_vec(),
                    keys_for_xor[i].key.as_bytes().to_vec(),
                )),
            ));
        }

        DataKeys::new(
            String::from(pqkd.sae_id()),
            String::from(pqkd.remote_sae_id()),
            Vec::from(path),
            keys_for_send,
        )
    };
    let request = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(format!("{}/info_keys", pqkd.remote_proxy_address()))
        .header("content-type", "application/json")
        .body(Body::new(serde_json::to_string(&data).unwrap()))
        .unwrap();

    let _ = client.request(request).await.unwrap().into_response();

    Ok(())
}

async fn get_keys(
    sae_id: &str,
    state: &AppStateRelay,
    payload_keys: &Vec<Prom>,
) -> Result<Vec<Key>, StatusCode> {
    let mut keys: Vec<Key> = Vec::new();

    let pqkd = state.pqkd(|p| p.sae_id() == sae_id).unwrap();
    let client = state.client(sae_id).unwrap();

    for key in payload_keys {
        match (key.key_id(), key.key_id_xor(), key.key()) {
            // jesli proxy przekazuje kluczy proxy obok
            (k_id, None, Some(k)) => {
                keys.push(Key {
                    key_id: String::from(k_id),
                    key: String::from_utf8(k.clone()).unwrap(),
                });
            }
            // jesli wysyla pierwszy wezel
            (k_id, None, None) => {
                let request = hyper::Request::builder()
                    .method(hyper::Method::GET)
                    .uri(format!(
                        "{}/api/v1/keys/{}/dec_keys?key_ID={}",
                        pqkd.kme_address(),
                        pqkd.remote_sae_id(),
                        k_id,
                    ))
                    .body(Body::empty())
                    .unwrap();

                let response = client.request(request).await.unwrap().into_response();
                let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let keys_from_pqkd: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
                let keys_from_pqkd = keys_from_pqkd.keys();
                let key_from_pqkd = keys_from_pqkd.first().unwrap();
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
                        pqkd.kme_address(),
                        pqkd.remote_sae_id(),
                        k_id_xor,
                    ))
                    .body(Body::empty())
                    .unwrap();

                let response = client.request(request).await.unwrap().into_response();
                let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let keys_from_pqkd: Keys = serde_json::from_slice(&body_bytes[..]).unwrap();
                let keys_from_pqkd = keys_from_pqkd.keys();
                let key_from_pqkd = keys_from_pqkd.first().unwrap();
                let key_before_xor = util::xor(k.clone(), key_from_pqkd.key.as_bytes().to_vec());
                let key_to_string = String::from_utf8(key_before_xor).unwrap();
                let k = Key {
                    key: key_to_string,
                    key_id: String::from(k_id),
                };
                keys.push(k);
            }
            _ => {}
        };
    }
    Ok(keys)
}
