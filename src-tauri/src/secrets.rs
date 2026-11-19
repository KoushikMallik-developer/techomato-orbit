//! Where secret environment values live. The real store is Windows Credential Manager
//! (added in a later task); the in-memory store backs tests.

use crate::error::AppResult;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn set(&self, service_id: &str, key: &str, value: &str) -> AppResult<()>;
    fn get(&self, service_id: &str, key: &str) -> AppResult<Option<String>>;
    fn delete(&self, service_id: &str, key: &str) -> AppResult<()>;
    fn delete_all(&self, service_id: &str) -> AppResult<()>;
}

#[derive(Default)]
pub struct InMemorySecretStore(Mutex<HashMap<(String, String), String>>);

impl SecretStore for InMemorySecretStore {
    fn set(&self, service_id: &str, key: &str, value: &str) -> AppResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert((service_id.into(), key.into()), value.into());
        Ok(())
    }

    fn get(&self, service_id: &str, key: &str) -> AppResult<Option<String>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(service_id.to_string(), key.to_string()))
            .cloned())
    }

    fn delete(&self, service_id: &str, key: &str) -> AppResult<()> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service_id.to_string(), key.to_string()));
        Ok(())
    }

    fn delete_all(&self, service_id: &str) -> AppResult<()> {
        self.0.lock().unwrap().retain(|(id, _), _| id != service_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_round_trips_and_deletes_per_service() {
        let s = InMemorySecretStore::default();
        s.set("a", "K", "v1").unwrap();
        s.set("a", "L", "v2").unwrap();
        s.set("b", "K", "v3").unwrap();
        assert_eq!(s.get("a", "K").unwrap().as_deref(), Some("v1"));
        s.delete_all("a").unwrap();
        assert_eq!(s.get("a", "K").unwrap(), None);
        assert_eq!(s.get("a", "L").unwrap(), None);
        assert_eq!(s.get("b", "K").unwrap().as_deref(), Some("v3"));
    }
}
