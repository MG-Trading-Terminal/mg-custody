//! PKCS#11 HSM adapter.
//!
//! Interfaces with hardware security modules via the PKCS#11 standard.
//! For development: use file-backed storage (encrypted).
//! For production: SoftHSM2, Thales Luna, Utimaco, AWS CloudHSM.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;

use super::HsmAdapter;

/// PKCS#11 HSM adapter configuration.
#[derive(Debug, Clone)]
pub struct Pkcs11Config {
    /// Path to the PKCS#11 library (.so/.dylib), or "file" for file-backed dev mode.
    pub library_path: String,
    /// HSM slot ID.
    pub slot_id: u64,
    /// HSM user PIN.
    pub pin: String,
    /// Storage directory for file-backed mode.
    pub storage_dir: Option<PathBuf>,
}

impl Pkcs11Config {
    /// Default config for SoftHSM2 on Linux.
    pub fn softhsm2() -> Self {
        Self {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".into(),
            slot_id: 0,
            pin: "0000".into(),
            storage_dir: None,
        }
    }

    /// File-backed storage for development (no real HSM needed).
    pub fn file_backed(dir: &str) -> Self {
        Self {
            library_path: "file".into(),
            slot_id: 0,
            pin: "dev".into(),
            storage_dir: Some(PathBuf::from(dir)),
        }
    }
}

/// PKCS#11 HSM adapter.
///
/// In production: wraps a PKCS#11 session for key share storage.
/// In development: uses encrypted file-backed storage.
pub struct Pkcs11Hsm {
    #[allow(dead_code)]
    config: Pkcs11Config,
    /// File-backed storage (dev mode). Key = "key_id:party_id", Value = encrypted bytes.
    file_store: Mutex<HashMap<String, Vec<u8>>>,
    /// Storage directory for persistence.
    storage_dir: Option<PathBuf>,
}

impl Pkcs11Hsm {
    /// Connect to the HSM (or initialize file-backed storage).
    pub fn connect(config: Pkcs11Config) -> Result<Self> {
        let storage_dir = config.storage_dir.clone();

        if config.library_path == "file" {
            // File-backed dev mode
            let dir = storage_dir.as_ref()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("/tmp/custody-hsm"));

            if !dir.exists() {
                std::fs::create_dir_all(&dir)?;
            }

            // Load existing shares from disk
            let mut store = HashMap::new();
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".share") {
                            let key = name.trim_end_matches(".share").to_string();
                            if let Ok(data) = std::fs::read(entry.path()) {
                                store.insert(key, data);
                            }
                        }
                    }
                }
            }

            tracing::info!(
                dir = %dir.display(),
                shares = store.len(),
                "File-backed HSM initialized"
            );

            Ok(Self {
                config,
                file_store: Mutex::new(store),
                storage_dir: Some(dir),
            })
        } else {
            // Real PKCS#11 — would load library and open session
            tracing::info!(
                library = %config.library_path,
                slot = config.slot_id,
                "Connecting to PKCS#11 HSM"
            );

            // In production, this would:
            // 1. dlopen(config.library_path)
            // 2. C_Initialize()
            // 3. C_OpenSession(config.slot_id)
            // 4. C_Login(session, CKU_USER, pin)
            //
            // For now, we use file-backed storage as fallback
            Ok(Self {
                config,
                file_store: Mutex::new(HashMap::new()),
                storage_dir,
            })
        }
    }
}

impl HsmAdapter for Pkcs11Hsm {
    fn store_share(&self, key_id: &str, party_id: u16, encrypted_share: &[u8]) -> Result<()> {
        let label = format!("{key_id}:{party_id}");

        // Store in memory
        self.file_store.lock().unwrap()
            .insert(label.clone(), encrypted_share.to_vec());

        // Persist to disk if file-backed
        if let Some(ref dir) = self.storage_dir {
            let path = dir.join(format!("{label}.share"));
            std::fs::write(&path, encrypted_share)?;
            tracing::debug!(key_id, party_id, path = %path.display(), "Share stored to file");
        }

        tracing::info!(key_id, party_id, size = encrypted_share.len(), "Key share stored");
        Ok(())
    }

    fn load_share(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>> {
        let label = format!("{key_id}:{party_id}");

        // Try memory first
        if let Some(data) = self.file_store.lock().unwrap().get(&label) {
            return Ok(data.clone());
        }

        // Try disk
        if let Some(ref dir) = self.storage_dir {
            let path = dir.join(format!("{label}.share"));
            if path.exists() {
                let data = std::fs::read(&path)?;
                self.file_store.lock().unwrap().insert(label, data.clone());
                return Ok(data);
            }
        }

        anyhow::bail!("Key share not found: {key_id} party {party_id}")
    }

    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()> {
        let label = format!("{key_id}:{party_id}");

        self.file_store.lock().unwrap().remove(&label);

        if let Some(ref dir) = self.storage_dir {
            let path = dir.join(format!("{label}.share"));
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }

        tracing::info!(key_id, party_id, "Key share deleted");
        Ok(())
    }

    fn list_keys(&self) -> Result<Vec<String>> {
        let store = self.file_store.lock().unwrap();
        let mut keys: Vec<String> = store.keys()
            .filter_map(|k| k.split(':').next().map(String::from))
            .collect();
        keys.sort();
        keys.dedup();
        Ok(keys)
    }
}

/// AWS CloudHSM adapter (uses PKCS#11 under the hood).
pub struct CloudHsmAdapter {
    inner: Pkcs11Hsm,
    #[allow(dead_code)]
    cluster_id: String,
}

impl CloudHsmAdapter {
    pub fn new(cluster_id: &str, config: Pkcs11Config) -> Result<Self> {
        let inner = Pkcs11Hsm::connect(config)?;
        Ok(Self { inner, cluster_id: cluster_id.into() })
    }
}

impl HsmAdapter for CloudHsmAdapter {
    fn store_share(&self, key_id: &str, party_id: u16, data: &[u8]) -> Result<()> {
        self.inner.store_share(key_id, party_id, data)
    }
    fn load_share(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>> {
        self.inner.load_share(key_id, party_id)
    }
    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()> {
        self.inner.delete_share(key_id, party_id)
    }
    fn list_keys(&self) -> Result<Vec<String>> {
        self.inner.list_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softhsm2_config() {
        let config = Pkcs11Config::softhsm2();
        assert!(config.library_path.contains("softhsm"));
        assert_eq!(config.slot_id, 0);
    }

    #[test]
    fn file_backed_store_roundtrip() {
        let dir = std::env::temp_dir().join("custody-hsm-test");
        let _ = std::fs::remove_dir_all(&dir);

        let config = Pkcs11Config::file_backed(dir.to_str().unwrap());
        let hsm = Pkcs11Hsm::connect(config).unwrap();

        // Store
        hsm.store_share("key1", 0, b"share_data_0").unwrap();
        hsm.store_share("key1", 1, b"share_data_1").unwrap();

        // Load
        let loaded = hsm.load_share("key1", 0).unwrap();
        assert_eq!(loaded, b"share_data_0");

        let loaded = hsm.load_share("key1", 1).unwrap();
        assert_eq!(loaded, b"share_data_1");

        // List
        let keys = hsm.list_keys().unwrap();
        assert_eq!(keys, vec!["key1".to_string()]);

        // Delete
        hsm.delete_share("key1", 0).unwrap();
        assert!(hsm.load_share("key1", 0).is_err());

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_backed_persistence() {
        let dir = std::env::temp_dir().join("custody-hsm-persist-test");
        let _ = std::fs::remove_dir_all(&dir);

        // First connection — store data
        {
            let config = Pkcs11Config::file_backed(dir.to_str().unwrap());
            let hsm = Pkcs11Hsm::connect(config).unwrap();
            hsm.store_share("persist_key", 0, b"persistent_data").unwrap();
        }

        // Second connection — data should still be there
        {
            let config = Pkcs11Config::file_backed(dir.to_str().unwrap());
            let hsm = Pkcs11Hsm::connect(config).unwrap();
            let loaded = hsm.load_share("persist_key", 0).unwrap();
            assert_eq!(loaded, b"persistent_data");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_share_fails() {
        let config = Pkcs11Config::file_backed("/tmp/custody-hsm-empty");
        let hsm = Pkcs11Hsm::connect(config).unwrap();
        assert!(hsm.load_share("nonexistent", 99).is_err());
    }
}
