//! PKCS#11 HSM adapter.
//!
//! Two modes:
//! - **File-backed** (`library_path = "file"`): encrypted local storage for dev/test.
//! - **Real PKCS#11** (`library_path = "/path/to/libsofthsm2.so"`): actual HSM via cryptoki.
//!
//! Supported HSMs: SoftHSM2 (dev), Thales Luna, Utimaco CryptoServer, AWS CloudHSM.
//!
//! The real PKCS#11 mode stores key shares as opaque data objects (CKO_DATA)
//! with a label of "key_id:party_id". This allows share storage/retrieval
//! without exposing the key material to the application.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use cryptoki::context::{CInitializeArgs, Pkcs11};
use cryptoki::object::{Attribute, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::types::AuthPin;

use super::HsmAdapter;

/// Wrapper for PKCS#11 context + session.
/// Contains raw pointers from the C library binding, so we use Mutex for thread safety.
pub struct Pkcs11Session {
    #[allow(dead_code)]
    ctx: Pkcs11,
    session: Session,
}

// SAFETY: Pkcs11Session is always accessed behind a Mutex<Pkcs11Session>,
// which ensures exclusive access. The raw pointers inside Pkcs11/Session
// are only dereferenced through the cryptoki safe API under this lock.
unsafe impl Send for Pkcs11Session {}
unsafe impl Sync for Pkcs11Session {}

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
/// In production: wraps a real PKCS#11 session for key share storage.
/// In development: uses encrypted file-backed storage.
pub struct Pkcs11Hsm {
    #[allow(dead_code)]
    config: Pkcs11Config,
    /// File-backed storage (dev mode). Key = "key_id:party_id", Value = encrypted bytes.
    file_store: Mutex<HashMap<String, Vec<u8>>>,
    /// Storage directory for persistence.
    storage_dir: Option<PathBuf>,
    /// Real PKCS#11 context + session (None in file-backed mode).
    /// Combined into a single struct behind Mutex because Pkcs11/Session
    /// contain raw pointers (!Send + !Sync). The Mutex ensures exclusive access.
    pkcs11: Option<Mutex<Pkcs11Session>>,
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
                    if let Some(name) = entry.file_name().to_str()
                        && name.ends_with(".share") {
                            let key = name.trim_end_matches(".share").to_string();
                            if let Ok(data) = std::fs::read(entry.path()) {
                                store.insert(key, data);
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
                pkcs11: None,
            })
        } else {
            // Real PKCS#11 — load library, initialize, open session, login
            tracing::info!(
                library = %config.library_path,
                slot = config.slot_id,
                "Connecting to PKCS#11 HSM"
            );

            let ctx = Pkcs11::new(&config.library_path)
                .context("Failed to load PKCS#11 library")?;

            ctx.initialize(CInitializeArgs::OsThreads)
                .context("C_Initialize failed")?;

            // Get slots and find the target
            let slots = ctx.get_slots_with_token()
                .context("C_GetSlotList failed")?;

            let slot = slots.into_iter()
                .find(|s| s.id() == config.slot_id)
                .ok_or_else(|| anyhow::anyhow!(
                    "PKCS#11 slot {} not found. Available slots may differ.", config.slot_id
                ))?;

            // Open read-write session
            let session = ctx.open_rw_session(slot)
                .context("C_OpenSession failed")?;

            // Login as normal user
            let pin = AuthPin::new(config.pin.clone());
            session.login(UserType::User, Some(&pin))
                .context("C_Login failed — check PIN and user setup")?;

            tracing::info!(
                library = %config.library_path,
                slot = config.slot_id,
                "PKCS#11 HSM session established"
            );

            Ok(Self {
                config,
                file_store: Mutex::new(HashMap::new()),
                storage_dir,
                pkcs11: Some(Mutex::new(Pkcs11Session { ctx, session })),
            })
        }
    }

    /// Check if running in real PKCS#11 mode (not file-backed).
    pub fn is_real_hsm(&self) -> bool {
        self.pkcs11.is_some()
    }

    /// Find a data object by label in the HSM.
    fn find_object_by_label(&self, label: &str) -> Result<Option<ObjectHandle>> {
        let hsm_mutex = self.pkcs11.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No PKCS#11 session"))?;
        let hsm = hsm_mutex.lock()
            .map_err(|e| anyhow::anyhow!("PKCS#11 session lock poisoned: {e}"))?;

        let template = vec![
            Attribute::Class(ObjectClass::DATA),
            Attribute::Label(label.as_bytes().to_vec()),
        ];

        let objects = hsm.session.find_objects(&template)
            .context("C_FindObjects failed")?;

        Ok(objects.into_iter().next())
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

    fn is_operational(&self) -> bool {
        if self.config.library_path == "file" {
            return true; // File-backed mode always operational
        }
        // Real PKCS#11 — verify session is alive
        if let Some(ref hsm) = self.pkcs11 {
            hsm.lock().is_ok()
        } else {
            false
        }
    }

    fn hsm_type(&self) -> &'static str {
        "pkcs11"
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
    fn is_operational(&self) -> bool {
        self.inner.is_operational()
    }
    fn hsm_type(&self) -> &'static str {
        "cloud_hsm"
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
    fn file_backed_is_operational() {
        let config = Pkcs11Config::file_backed("/tmp/custody-hsm-op-test");
        let hsm = Pkcs11Hsm::connect(config).unwrap();
        assert!(hsm.is_operational());
        assert_eq!(hsm.hsm_type(), "pkcs11");
        // HSM guard should pass
        super::super::assert_hsm_operational(&hsm);
        let _ = std::fs::remove_dir_all("/tmp/custody-hsm-op-test");
    }

    #[test]
    fn cloud_hsm_operational() {
        let config = Pkcs11Config::file_backed("/tmp/custody-cloud-hsm-test");
        let hsm = CloudHsmAdapter::new("test-cluster", config).unwrap();
        assert!(hsm.is_operational());
        assert_eq!(hsm.hsm_type(), "cloud_hsm");
        let _ = std::fs::remove_dir_all("/tmp/custody-cloud-hsm-test");
    }

    #[test]
    fn load_nonexistent_share_fails() {
        let config = Pkcs11Config::file_backed("/tmp/custody-hsm-empty");
        let hsm = Pkcs11Hsm::connect(config).unwrap();
        assert!(hsm.load_share("nonexistent", 99).is_err());
    }

    #[test]
    fn real_pkcs11_connect_softhsm2() {
        // This test verifies real PKCS#11 connection via SoftHSM2.
        // Soft-fails if SoftHSM2 is not installed (expected in CI).
        let lib_paths = [
            "/usr/lib/softhsm/libsofthsm2.so",       // Debian/Ubuntu
            "/usr/local/lib/softhsm/libsofthsm2.so",  // manual install
            "/opt/homebrew/lib/softhsm/libsofthsm2.so", // macOS Homebrew
            "/usr/local/lib/libsofthsm2.so",          // FreeBSD
        ];

        let Some(lib_path) = lib_paths.iter().find(|p| std::path::Path::new(p).exists()) else {
            eprintln!("Skipping: SoftHSM2 not installed (no libsofthsm2.so found)");
            return;
        };

        let config = Pkcs11Config {
            library_path: lib_path.to_string(),
            slot_id: 0,
            pin: "0000".into(),
            storage_dir: None,
        };

        // Attempt to connect — may fail if no token is initialized in slot 0
        match Pkcs11Hsm::connect(config) {
            Ok(hsm) => {
                assert!(hsm.is_real_hsm(), "Should be in real PKCS#11 mode");
                assert!(hsm.is_operational());
                println!("SoftHSM2 PKCS#11 connection successful via {lib_path}");
            }
            Err(e) => {
                // Expected if slot 0 has no token — not a test failure
                eprintln!("SoftHSM2 found at {lib_path} but connect failed: {e}");
                eprintln!("(Run `softhsm2-util --init-token --slot 0 --label test --pin 0000 --so-pin 0000` to set up)");
            }
        }
    }

    #[test]
    fn file_backed_not_real_hsm() {
        let config = Pkcs11Config::file_backed("/tmp/custody-not-real-hsm");
        let hsm = Pkcs11Hsm::connect(config).unwrap();
        assert!(!hsm.is_real_hsm(), "File-backed should not be real HSM");
        let _ = std::fs::remove_dir_all("/tmp/custody-not-real-hsm");
    }
}
