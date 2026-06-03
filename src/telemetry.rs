use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::config::TelemetryConfig;

#[derive(Serialize, Clone)]
pub struct PqkdTelemetryPair {
    sae_id: String,
    paired_with: String,
}

impl PqkdTelemetryPair {
    pub fn new(sae_id: String, paired_with: String) -> Self {
        Self {
            sae_id,
            paired_with,
        }
    }
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
    pqkds: Vec<PqkdTelemetryPair>,
    connections: Vec<TopologyEdge>,
    timestamp_utc: String,
}

#[derive(Serialize)]
struct HeartbeatEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    network_id: Option<String>,
    relay_id: String,
    pqkds: Vec<PqkdTelemetryPair>,
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
    pqkds: Vec<PqkdTelemetryPair>,
    connections: Vec<TopologyEdge>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !config.enabled() {
            tracing::info!("Telemetry disabled");
            return;
        }

        let ws_url = config.server_ws_url().to_string();
        if !validate_ws_url(&ws_url) {
            tracing::error!("Telemetry ws url must start with ws:// or wss://: {}", ws_url);
            return;
        }

        let interval_sec = config.interval_sec().max(1);
        let interval_duration = Duration::from_secs(interval_sec);

        loop {
            match connect_async(&ws_url).await {
                Ok((socket, _)) => {
                    tracing::info!("Telemetry connected to {}", ws_url);
                    let (mut writer, mut reader) = socket.split();

                    let register = RegisterEvent {
                        event_type: "pqkd-relay.register",
                        network_id: network_id.clone(),
                        relay_id: relay_id.clone(),
                        pqkds: pqkds.clone(),
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
                                let heartbeat = HeartbeatEvent {
                                    event_type: "pqkd-relay.heartbeat",
                                    network_id: network_id.clone(),
                                    relay_id: relay_id.clone(),
                                    pqkds: pqkds.clone(),
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
