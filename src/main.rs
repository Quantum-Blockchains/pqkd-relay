use hyper::client;
use pqkd_relay::app_state::{AppState, AppStateEtsi, AppStateRelay};
use pqkd_relay::config::Config;
use pqkd_relay::{cli, etsi_server};
use pqkd_relay::{etsi_server::EtsiServer, relay_server::RelayServer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = cli::Args::fron_args();
    let config = Config::build(args.config_file).unwrap();

    let mut list_handles = Vec::new();

    let mut keys_map = HashMap::new();
    let mut clients_map = HashMap::new();

    for pqkd in config.pqkds() {
        let keys = Arc::new(Mutex::new(Vec::new()));
        keys_map.insert(pqkd.sae_id().to_string(), Arc::clone(&keys));
        let app_state_etsi = AppStateEtsi::build(pqkd.sae_id(), &config, keys);
        clients_map.insert(
            pqkd.sae_id().to_string(),
            Arc::clone(app_state_etsi.client()),
        );

        let etsi_server = EtsiServer::build(app_state_etsi, &pqkd).await;

        let handle = tokio::task::spawn(async {
            etsi_server.run().await.unwrap();
        });

        list_handles.push(handle);

        println!("ETSI {}: 0.0.0.0:{:?}", pqkd.sae_id(), pqkd.port());
    }

    //let app_state = Arc::new(AppState::build(&config));

    //let etsi_server = EtsiServer::build(Arc::clone(&app_state), &config).await;

    let app_state_relay = AppStateRelay::build(config.pqkds().clone(), clients_map, keys_map);

    let relay_server = RelayServer::build(app_state_relay, &config).await;

    //let a = tokio::task::spawn(async {
    //  etsi_server.run().await.unwrap();
    //});
    let handle_relay = tokio::task::spawn(async {
        relay_server.run().await.unwrap();
    });

    list_handles.push(handle_relay);

    println!("PROXY: 0.0.0.0:{:?}", config.port());

    let mut results = Vec::with_capacity(list_handles.len());

    for handle in list_handles {
        results.push(handle.await.unwrap());
    }

    //handle_relay.await.unwrap();

    //list_handles.iter().map(|h| tokio::join!(h)).collect();
    //let res = tokio::join!(handle_relay);
    //handle_relay.join().unwrap();

    //let (res_a, res_b) = tokio::join!(a, b);

    //res_a.unwrap();
    //res_b.unwrap();

    //res.unwrap();

    Ok(())
}
