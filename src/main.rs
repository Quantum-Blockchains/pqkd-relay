use pqkd_relay::app_state::AppState;
use pqkd_relay::cli;
use pqkd_relay::config::Config;
use pqkd_relay::{etsi_server::EtsiServer, relay_server::RelayServer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = cli::Args::fron_args();
    let config = Config::build(args.config_file).unwrap();

    let app_state = Arc::new(AppState::build(&config));

    let etsi_server = EtsiServer::build(Arc::clone(&app_state), &config).await;
    let relay_server = RelayServer::build(app_state, &config).await;

    let a = tokio::task::spawn(async {
        etsi_server.run().await.unwrap();
    });
    let b = tokio::task::spawn(async {
        relay_server.run().await.unwrap();
    });

    println!("ETSI: 127.0.0.1:{:?}", config.port_etsi());
    println!("PROXY: 127.0.0.1:{:?}", config.port());

    let (res_a, res_b) = tokio::join!(a, b);

    res_a.unwrap();
    res_b.unwrap();

    Ok(())
}
