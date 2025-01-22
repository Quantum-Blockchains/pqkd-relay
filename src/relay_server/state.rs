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

        let is_save = keys.iter().find(|k| k.from == from && k.key_id == key_id);

        if let Some(k) = is_save {
            if k.key == key {
                Ok(())
            } else {
                Err(RelayServerError::KeysDoNotMaych)
            }
        } else {
            keys.push(KeyReceived::new(from, key_id, key));
            Ok(())
        }
    }

    // pub fn get_key(
    //     &self,
    //     sae_id: &str,
    //     from: &str,
    //     key_ids: &KeyIds,
    // ) -> Result<Keys, RelayServerError> {
    //     let mut keys = self
    //         .keys
    //         .get(sae_id)
    //         .ok_or(RelayServerError::GetKeysError)?
    //         .lock()
    //         .map_err(|_| RelayServerError::GetKeysError)?;
    //     let mut return_keys = Vec::new();
    //     key_ids.key_ids.iter().map(|key_id| {
    //         keys.iter()
    //             .position(|k| k.from == from && k.key_id == key_id.key_id)
    //             .map(|i| {
    //                 let key = keys.swap_remove(i);
    //                 return_keys.push(Key {
    //                     key: key.key,
    //                     key_id: key.key_id,
    //                 })
    //             })
    //     });
    //     Ok(Keys { keys: return_keys })
    // }
}
