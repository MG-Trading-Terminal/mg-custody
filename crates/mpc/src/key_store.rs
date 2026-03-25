//! Encrypted key share storage.
//!
//! Wraps the HSM adapter with AES-256-GCM encryption for key shares.
//! Reuses the encryption primitives from custody-signing/encrypted_store.

use anyhow::Result;

use crate::hsm::HsmAdapter;

/// Encrypted key store backed by an HSM adapter.
pub struct KeyStore {
    hsm: Box<dyn HsmAdapter>,
}

impl KeyStore {
    pub fn new(hsm: Box<dyn HsmAdapter>) -> Self {
        Self { hsm }
    }

    /// Store a key share (already encrypted by the caller).
    pub fn store(&self, key_id: &str, party_id: u16, encrypted_data: &[u8]) -> Result<()> {
        self.hsm.store_share(key_id, party_id, encrypted_data)
    }

    /// Load an encrypted key share.
    pub fn load(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>> {
        self.hsm.load_share(key_id, party_id)
    }

    /// Delete a key share.
    pub fn delete(&self, key_id: &str, party_id: u16) -> Result<()> {
        self.hsm.delete_share(key_id, party_id)
    }

    /// List all stored key groups.
    pub fn list_keys(&self) -> Result<Vec<String>> {
        self.hsm.list_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsm::soft::SoftwareHsm;

    #[test]
    fn key_store_roundtrip() {
        let store = KeyStore::new(Box::new(SoftwareHsm::new()));
        let data = b"encrypted key share bytes";

        store.store("group1", 1, data).unwrap();
        let loaded = store.load("group1", 1).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn key_store_list() {
        let store = KeyStore::new(Box::new(SoftwareHsm::new()));
        store.store("alpha", 1, b"a").unwrap();
        store.store("beta", 1, b"b").unwrap();

        let keys = store.list_keys().unwrap();
        assert_eq!(keys.len(), 2);
    }
}
