use crate::config::Pqkd;
use crate::etsi_server::{Client, KeyReceived};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::error::RelayServerError;

#[derive(Clone)]
pub struct AppStateRelay {
    pqkds: Vec<Pqkd>,
    clients: Arc<HashMap<String, Arc<Client>>>,
    keys: HashMap<String, Arc<Mutex<Vec<KeyReceived>>>>,
}

impl AppStateRelay {
    pub fn build(
        pqkds: Vec<Pqkd>,
        clients: Arc<HashMap<String, Arc<Client>>>,
        keys: HashMap<String, Arc<Mutex<Vec<KeyReceived>>>>,
    ) -> AppStateRelay {
        AppStateRelay {
            pqkds,
            clients,
            keys,
        }
    }

    // pub fn pqkd(&self, sae_id: &str) -> Option<&Pqkd> {
    //     self.pqkds.iter().find(|p| p.sae_id() == sae_id)
    // }

    pub fn pqkd<P>(&self, predicate: P) -> Option<&Pqkd>
    where
        P: FnMut(&&Pqkd) -> bool,
    {
        self.pqkds.iter().find(predicate)
    }

    pub fn client(&self, sae_id: &str) -> Option<&Arc<Client>> {
        self.clients.get(sae_id)
    }

    pub fn add_key(
        &self,
        sae_id: &str,
        from: String,
        key_id: String,
        key: String,
    ) -> Result<(), RelayServerError> {
        let mut keys = self
            .keys
            .get(sae_id)
            .ok_or(RelayServerError::AddKeyError)?
            .lock()
            .map_err(|_| RelayServerError::AddKeyError)?;

        let is_save = keys
            .iter()
            .position(|k| k.from == from && k.key_id == key_id);

        if let Some(p) = is_save {
            if keys[p].key == key {
                keys[p].num();
                Ok(())
            } else {
                Err(RelayServerError::KeysDoNotMaych)
            }
        } else {
            keys.push(KeyReceived::new(from, key_id, key));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppStateRelay;
    use crate::config::Config;
    use crate::relay_server::error::RelayServerError;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn test_config() -> Config {
        let toml = r#"
id = "00"
port = 4000

[[pqkds]]
port = 3000
sae_id = "Alice"
remote_sae_id = "Bob"
remote_proxy_address = "http://127.0.0.1:4001"
kme_address = "http://127.0.0.1:8080"
"#;

        toml::from_str(toml).expect("valid config")
    }

    #[test]
    fn add_key_creates_new_entry_then_increments_counter_on_duplicate() {
        let config = test_config();
        let key_store = Arc::new(Mutex::new(Vec::new()));
        let state = AppStateRelay::build(
            config.pqkds().clone(),
            Arc::new(HashMap::new()),
            HashMap::from([("Alice".to_string(), Arc::clone(&key_store))]),
        );

        state
            .add_key(
                "Alice",
                "Relay_00".to_string(),
                "key-1".to_string(),
                "value-1".to_string(),
            )
            .expect("first save should succeed");

        state
            .add_key(
                "Alice",
                "Relay_00".to_string(),
                "key-1".to_string(),
                "value-1".to_string(),
            )
            .expect("duplicate same key should increment counter");

        let stored = key_store.lock().expect("key store lock");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].num, 2);
    }

    #[test]
    fn add_key_returns_error_when_same_key_id_has_different_payload() {
        let config = test_config();
        let key_store = Arc::new(Mutex::new(Vec::new()));
        let state = AppStateRelay::build(
            config.pqkds().clone(),
            Arc::new(HashMap::new()),
            HashMap::from([("Alice".to_string(), key_store)]),
        );

        state
            .add_key(
                "Alice",
                "Relay_00".to_string(),
                "key-1".to_string(),
                "value-1".to_string(),
            )
            .expect("first add should pass");

        let err = state
            .add_key(
                "Alice",
                "Relay_00".to_string(),
                "key-1".to_string(),
                "different".to_string(),
            )
            .expect_err("mismatch must fail");

        assert!(matches!(err, RelayServerError::KeysDoNotMaych));
    }
}
