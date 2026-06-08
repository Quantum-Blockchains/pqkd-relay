use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::config::TelemetryConfig;

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PqkdStatus {
    Ok,
    Error,
    Unknown,
}

pub struct PqkdEntry {
    pub sae_id: String,
    pub paired_with: String,
    pub kme_address: String,
}

impl PqkdEntry {
    pub fn new(sae_id: String, paired_with: String, kme_address: String) -> Self {
        Self {
            sae_id,
            paired_with,
            kme_address,
        }
    }
}

#[derive(Serialize)]
struct PqkdEventPair {
    sae_id: String,
    paired_with: String,
    status: PqkdStatus,
}

#[derive(Serialize, Clone)]
pub struct TopologyEdge {
    first: String,
    second: String,
}

impl TopologyEdge {
    pub fn new(first: String, second: String) -> Self {
        Self { first, second }
    }
}

#[derive(Serialize)]
struct RegisterEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    network_id: Option<String>,
    relay_id: String,
    pqkds: Vec<PqkdEventPair>,
    connections: Vec<TopologyEdge>,
    timestamp_utc: String,
}

#[derive(Serialize)]
struct HeartbeatEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    network_id: Option<String>,
    relay_id: String,
    pqkds: Vec<PqkdEventPair>,
    timestamp_utc: String,
}

fn now_utc() -> String {
    Utc::now().to_rfc3339()
}

fn validate_ws_url(url: &str) -> bool {
    url.starts_with("ws://") || url.starts_with("wss://")
}

pub fn spawn(
    config: TelemetryConfig,
    network_id: Option<String>,
    relay_id: String,
    pqkd_entries: Vec<PqkdEntry>,
    connections: Vec<TopologyEdge>,
    clients: Arc<HashMap<String, Arc<crate::Client>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !config.enabled() {
            tracing::info!("Telemetry disabled");
            return;
        }

        let ws_url = config.server_ws_url().to_string();
        if !validate_ws_url(&ws_url) {
            tracing::error!(
                "Telemetry ws url must start with ws:// or wss://: {}",
                ws_url
            );
            return;
        }

        let interval_sec = config.interval_sec().max(1);
        let interval_duration = Duration::from_secs(interval_sec);

        // Shared status per PQKD — written by health-check tasks, read by heartbeat.
        let statuses: Vec<Arc<RwLock<PqkdStatus>>> = pqkd_entries
            .iter()
            .map(|_| Arc::new(RwLock::new(PqkdStatus::Unknown)))
            .collect();

        // Health-check tasks — spawned once, outside the WS reconnect loop.
        for (entry, status) in pqkd_entries.iter().zip(statuses.iter()) {
            let status = Arc::clone(status);
            let client = clients.get(&entry.sae_id).cloned();
            let url = format!(
                "{}/api/v1/keys/{}/status",
                entry.kme_address, entry.paired_with
            );
            tokio::spawn(async move {
                loop {
                    let new_status = match client {
                        Some(ref c) => {
                            let req = hyper::Request::builder()
                                .method(hyper::Method::GET)
                                .uri(&url)
                                .body(axum::body::Body::empty());
                            match req {
                                Ok(req) => match c.request(req).await {
                                    Ok(resp) => {
                                        if resp.status().is_success() {
                                            PqkdStatus::Ok
                                        } else {
                                            PqkdStatus::Error
                                        }
                                    }
                                    Err(_) => PqkdStatus::Error,
                                },
                                Err(_) => PqkdStatus::Error,
                            }
                        }
                        None => PqkdStatus::Error,
                    };
                    *status.write().await = new_status;
                    tokio::time::sleep(interval_duration).await;
                }
            });
        }

        loop {
            match connect_async(&ws_url).await {
                Ok((socket, _)) => {
                    tracing::info!("Telemetry connected to {}", ws_url);
                    let (mut writer, mut reader) = socket.split();

                    let pqkd_pairs = read_pqkd_pairs(&pqkd_entries, &statuses).await;
                    let register = RegisterEvent {
                        event_type: "pqkd-relay.register",
                        network_id: network_id.clone(),
                        relay_id: relay_id.clone(),
                        pqkds: pqkd_pairs,
                        connections: connections.clone(),
                        timestamp_utc: now_utc(),
                    };
                    match serde_json::to_string(&register) {
                        Ok(payload) => {
                            if let Err(err) = writer.send(Message::Text(payload)).await {
                                tracing::warn!("Telemetry register send failed: {}", err);
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                continue;
                            }
                        }
                        Err(err) => {
                            tracing::warn!("Telemetry register encode failed: {}", err);
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                    }

                    let mut ticker = tokio::time::interval(interval_duration);
                    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            _ = ticker.tick() => {
                                let pqkd_pairs = read_pqkd_pairs(&pqkd_entries, &statuses).await;
                                let heartbeat = HeartbeatEvent {
                                    event_type: "pqkd-relay.heartbeat",
                                    network_id: network_id.clone(),
                                    relay_id: relay_id.clone(),
                                    pqkds: pqkd_pairs,
                                    timestamp_utc: now_utc(),
                                };

                                match serde_json::to_string(&heartbeat) {
                                    Ok(payload) => {
                                        if let Err(err) = writer.send(Message::Text(payload)).await {
                                            tracing::warn!("Telemetry heartbeat send failed: {}", err);
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        tracing::warn!("Telemetry heartbeat encode failed: {}", err);
                                    }
                                }
                            }
                            next = reader.next() => {
                                match next {
                                    Some(Ok(Message::Close(_))) => {
                                        tracing::warn!("Telemetry websocket closed by server");
                                        break;
                                    }
                                    Some(Ok(_)) => {}
                                    Some(Err(err)) => {
                                        tracing::warn!("Telemetry websocket read error: {}", err);
                                        break;
                                    }
                                    None => {
                                        tracing::warn!("Telemetry websocket stream ended");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!("Telemetry connect failed: {}", err);
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

async fn read_pqkd_pairs(
    entries: &[PqkdEntry],
    statuses: &[Arc<RwLock<PqkdStatus>>],
) -> Vec<PqkdEventPair> {
    let mut pairs = Vec::with_capacity(entries.len());
    for (entry, status) in entries.iter().zip(statuses.iter()) {
        pairs.push(PqkdEventPair {
            sae_id: entry.sae_id.clone(),
            paired_with: entry.paired_with.clone(),
            status: *status.read().await,
        });
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pqkd_status_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&PqkdStatus::Ok).unwrap(), r#""ok""#);
        assert_eq!(
            serde_json::to_string(&PqkdStatus::Error).unwrap(),
            r#""error""#
        );
        assert_eq!(
            serde_json::to_string(&PqkdStatus::Unknown).unwrap(),
            r#""unknown""#
        );
    }

    #[test]
    fn pqkd_event_pair_includes_status_field() {
        let pair = PqkdEventPair {
            sae_id: "ab".to_string(),
            paired_with: "ba".to_string(),
            status: PqkdStatus::Ok,
        };
        let json = serde_json::to_string(&pair).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""sae_id":"ab""#));
        assert!(json.contains(r#""paired_with":"ba""#));
    }
}
