//! API keys live only in the macOS Keychain (requirement BR-6, `privacy-security` steering).
//!
//! [`SecretStore`] abstracts the store so the registry and commands are testable with [`MemoryStore`].

use std::collections::HashMap;
use std::sync::Mutex;

/// Keychain service name for every BOB secret.
pub const SERVICE: &str = "dev.bob.assistant";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("Keychain error: {0}")]
    Store(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, brain_id: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, brain_id: &str, key: &str) -> Result<(), SecretError>;
    fn delete(&self, brain_id: &str) -> Result<(), SecretError>;
}

/// Keychain account name for a brain's API key.
pub fn account(brain_id: &str) -> String {
    format!("brain-api-key:{brain_id}")
}

/// The real store: macOS Keychain via the `keyring` crate (`apple-native` backend).
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn get(&self, brain_id: &str) -> Result<Option<String>, SecretError> {
        let entry = keyring::Entry::new(SERVICE, &account(brain_id))
            .map_err(|e| SecretError::Store(e.to_string()))?;
        match entry.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }

    fn set(&self, brain_id: &str, key: &str) -> Result<(), SecretError> {
        keyring::Entry::new(SERVICE, &account(brain_id))
            .and_then(|e| e.set_password(key))
            .map_err(|e| SecretError::Store(e.to_string()))
    }

    fn delete(&self, brain_id: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(SERVICE, &account(brain_id))
            .map_err(|e| SecretError::Store(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, brain_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(brain_id)
            .cloned())
    }
    fn set(&self, brain_id: &str, key: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(brain_id.into(), key.into());
        Ok(())
    }
    fn delete(&self, brain_id: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(brain_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_deletes() {
        let s = MemoryStore::default();
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", "k1").unwrap();
        s.set("a", "k2").unwrap();
        assert_eq!(s.get("a").unwrap(), Some("k2".into()));
        s.delete("a").unwrap();
        s.delete("a").unwrap(); // idempotent
        assert_eq!(s.get("a").unwrap(), None);
    }

    #[test]
    fn keychain_accounts_are_namespaced_per_brain() {
        assert_eq!(account("ollama-1"), "brain-api-key:ollama-1");
    }

    /// Touches the real login Keychain, which can show a system prompt. Run manually:
    /// `cargo test keychain_round_trip -- --ignored`
    #[test]
    #[ignore = "uses the real macOS Keychain"]
    fn keychain_round_trip() {
        let id = format!("selftest-{}", std::process::id());
        let s = KeychainStore;
        s.set(&id, "not-a-real-key").unwrap();
        assert_eq!(s.get(&id).unwrap(), Some("not-a-real-key".into()));
        s.delete(&id).unwrap();
        assert_eq!(s.get(&id).unwrap(), None);
    }
}
