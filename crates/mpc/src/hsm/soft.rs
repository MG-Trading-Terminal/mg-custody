//! Software HSM for development and testing.
//!
//! Stores key shares in memory (HashMap). NOT for production.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

use super::HsmAdapter;

/// In-memory software HSM for development.
pub struct SoftwareHsm {
    shares: Mutex<HashMap<String, Vec<u8>>>,
}

impl SoftwareHsm {
    pub fn new() -> Self {
        Self {
            shares: Mutex::new(HashMap::new()),
        }
    }

    fn key(key_id: &str, party_id: u16) -> String {
        format!("{key_id}:{party_id}")
    }
}

impl Default for SoftwareHsm {
    fn default() -> Self {
        Self::new()
    }
}

impl HsmAdapter for SoftwareHsm {
    fn store_share(&self, key_id: &str, party_id: u16, encrypted_share: &[u8]) -> Result<()> {
        let mut shares = self.shares.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        shares.insert(Self::key(key_id, party_id), encrypted_share.to_vec());
        tracing::debug!(key_id, party_id, "Share stored in software HSM");
        Ok(())
    }

    fn load_share(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>> {
        let shares = self.shares.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let key = Self::key(key_id, party_id);
        shares
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Share not found: {key}"))
    }

    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()> {
        let mut shares = self.shares.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        shares.remove(&Self::key(key_id, party_id));
        Ok(())
    }

    fn list_keys(&self) -> Result<Vec<String>> {
        let shares = self.shares.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let keys: Vec<String> = shares
            .keys()
            .filter_map(|k| k.split(':').next().map(String::from))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_load_share() {
        let hsm = SoftwareHsm::new();
        let data = b"encrypted share data";
        hsm.store_share("key1", 1, data).unwrap();
        let loaded = hsm.load_share("key1", 1).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn load_missing_share_fails() {
        let hsm = SoftwareHsm::new();
        assert!(hsm.load_share("nonexistent", 1).is_err());
    }

    #[test]
    fn delete_share() {
        let hsm = SoftwareHsm::new();
        hsm.store_share("key1", 1, b"data").unwrap();
        hsm.delete_share("key1", 1).unwrap();
        assert!(hsm.load_share("key1", 1).is_err());
    }

    #[test]
    fn list_keys() {
        let hsm = SoftwareHsm::new();
        hsm.store_share("key1", 1, b"a").unwrap();
        hsm.store_share("key1", 2, b"b").unwrap();
        hsm.store_share("key2", 1, b"c").unwrap();
        let keys = hsm.list_keys().unwrap();
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }
}
