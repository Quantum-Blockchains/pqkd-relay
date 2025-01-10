use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
mod cli;
mod config;
mod etsi_server;
mod relay_server;
mod util;
use config::Config;
use etsi_server::{AppStateEtsi, EtsiServer};
use relay_server::{AppStateRelay, RelayServer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = cli::Args::fron_args();
    let config = Config::build(args.config_file).unwrap();

    let mut list_handles = Vec::new();

    let mut keys_map = HashMap::new();

    let mut clients_map = HashMap::new();

    for pqkd in config.pqkds() {
        let keys = Arc::new(Mutex::new(Vec::new()));
        keys_map.insert(pqkd.sae_id().to_string(), Arc::clone(&keys));
        let app_state_etsi = AppStateEtsi::build(pqkd.sae_id(), &config, keys)?;
        clients_map.insert(
            pqkd.sae_id().to_string(),
            Arc::clone(app_state_etsi.client()),
        );

        let etsi_server = EtsiServer::build(app_state_etsi, pqkd).await?;

        let handle = tokio::task::spawn(async {
            etsi_server.run().await.unwrap();
        });

        list_handles.push(handle);

        tracing::info!(
            "ETSI server for PQKD {} with address {} start: 0.0.0.0:{:?}",
            pqkd.sae_id(),
            pqkd.kme_address(),
            pqkd.port()
        );
    }

    let app_state_relay = AppStateRelay::build(config.pqkds().clone(), clients_map, keys_map);

    let relay_server = RelayServer::build(app_state_relay, &config).await;

    let handle_relay = tokio::task::spawn(async {
        relay_server.run().await.unwrap();
    });

    list_handles.push(handle_relay);

    tracing::info!("RELEY server start: 0.0.0.0:{:?}", config.port());

    //let mut results = Vec::with_capacity(list_handles.len());

    for handle in list_handles {
        handle.await.unwrap();
    }

    Ok(())
}
