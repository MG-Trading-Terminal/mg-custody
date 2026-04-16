//! MPC share persistence — survive process restarts.
//!
//! Key shares MUST persist across restarts. Otherwise, a restart during
//! a signing session means the share is lost and the key group is broken.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A persisted MPC key share (encrypted at rest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedShare {
    pub key_id: String,
    pub party_index: u16,
    pub threshold: u16,
    pub total_parties: u16,
    /// AES-256-GCM encrypted share data.
    pub encrypted_data: Vec<u8>,
    /// SHA-256 hash of the plaintext share (for integrity verification).
    pub data_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Decrypted share (zeroized on drop).
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DecryptedShare {
    pub data: Vec<u8>,
}

impl std::fmt::Debug for DecryptedShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DecryptedShare(***)")
    }
}

/// Share store trait — persist shares across restarts.
pub trait ShareStore: Send + Sync {
    fn save_share(&self, share: &PersistedShare) -> Result<()>;
    fn load_share(&self, key_id: &str, party_index: u16) -> Result<Option<PersistedShare>>;
    fn list_shares(&self) -> Result<Vec<PersistedShare>>;
    fn delete_share(&self, key_id: &str, party_index: u16) -> Result<()>;
}

/// File-based share store (for development).
pub struct FileShareStore {
    directory: std::path::PathBuf,
}

impl FileShareStore {
    pub fn new(directory: impl Into<std::path::PathBuf>) -> Self {
        let dir = directory.into();
        std::fs::create_dir_all(&dir).ok();
        Self { directory: dir }
    }

    fn share_path(&self, key_id: &str, party_index: u16) -> std::path::PathBuf {
        self.directory.join(format!("{key_id}_{party_index}.share"))
    }
}

impl ShareStore for FileShareStore {
    fn save_share(&self, share: &PersistedShare) -> Result<()> {
        let path = self.share_path(&share.key_id, share.party_index);
        let json = serde_json::to_string_pretty(share)?;
        std::fs::write(&path, json)?;
        tracing::info!(key_id = %share.key_id, party = share.party_index, "Share persisted to file");
        Ok(())
    }

    fn load_share(&self, key_id: &str, party_index: u16) -> Result<Option<PersistedShare>> {
        let path = self.share_path(key_id, party_index);
        if !path.exists() { return Ok(None); }
        let json = std::fs::read_to_string(&path)?;
        let share: PersistedShare = serde_json::from_str(&json)?;
        Ok(Some(share))
    }

    fn list_shares(&self) -> Result<Vec<PersistedShare>> {
        let mut shares = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.directory) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "share")
                    && let Ok(json) = std::fs::read_to_string(entry.path())
                        && let Ok(share) = serde_json::from_str(&json) {
                            shares.push(share);
                        }
            }
        }
        Ok(shares)
    }

    fn delete_share(&self, key_id: &str, party_index: u16) -> Result<()> {
        let path = self.share_path(key_id, party_index);
        if path.exists() { std::fs::remove_file(&path)?; }
        Ok(())
    }
}

/// Compute SHA-256 hash of share data for integrity check.
pub fn hash_share_data(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

/// Verify share integrity.
pub fn verify_share_integrity(share: &PersistedShare, decrypted: &[u8]) -> bool {
    hash_share_data(decrypted) == share.data_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_share_store_roundtrip() {
        let dir = std::env::temp_dir().join("custody_share_test");
        let store = FileShareStore::new(&dir);

        let share = PersistedShare {
            key_id: "test-key-1".into(),
            party_index: 0,
            threshold: 2, total_parties: 3,
            encrypted_data: vec![0xAA, 0xBB, 0xCC],
            data_hash: hash_share_data(&[0xDE, 0xAD]),
            created_at: Utc::now(),
            last_used_at: None,
        };

        store.save_share(&share).unwrap();
        let loaded = store.load_share("test-key-1", 0).unwrap().unwrap();
        assert_eq!(loaded.key_id, "test-key-1");
        assert_eq!(loaded.encrypted_data, vec![0xAA, 0xBB, 0xCC]);

        // Cleanup
        store.delete_share("test-key-1", 0).unwrap();
        assert!(store.load_share("test-key-1", 0).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let dir = std::env::temp_dir().join("custody_share_test_2");
        let store = FileShareStore::new(&dir);
        assert!(store.load_share("nope", 0).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hash_deterministic() {
        let h1 = hash_share_data(b"test data");
        let h2 = hash_share_data(b"test data");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_data() {
        assert_ne!(hash_share_data(b"data1"), hash_share_data(b"data2"));
    }

    #[test]
    fn verify_integrity_pass() {
        let data = b"my share";
        let share = PersistedShare {
            key_id: "k".into(), party_index: 0,
            threshold: 2, total_parties: 3,
            encrypted_data: vec![], data_hash: hash_share_data(data),
            created_at: Utc::now(), last_used_at: None,
        };
        assert!(verify_share_integrity(&share, data));
    }

    #[test]
    fn verify_integrity_fail() {
        let share = PersistedShare {
            key_id: "k".into(), party_index: 0,
            threshold: 2, total_parties: 3,
            encrypted_data: vec![], data_hash: hash_share_data(b"original"),
            created_at: Utc::now(), last_used_at: None,
        };
        assert!(!verify_share_integrity(&share, b"tampered"));
    }

    #[test]
    fn decrypted_share_debug_redacted() {
        let share = DecryptedShare { data: vec![0xFF; 32] };
        let debug = format!("{share:?}");
        assert!(debug.contains("***"));
        assert!(!debug.contains("ff"));
    }
}
