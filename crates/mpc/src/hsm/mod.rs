//! HSM adapter for key share storage.
//!
//! Provides a trait for storing MPC key shares in hardware security modules.
//! Implementations: software (dev), PKCS#11 (production), cloud HSM.

pub mod soft;

use anyhow::Result;

/// Trait for HSM-backed key share storage.
///
/// Key shares are stored encrypted. The HSM never exposes the raw share
/// outside its secure boundary.
pub trait HsmAdapter: Send + Sync {
    /// Store an encrypted key share.
    fn store_share(&self, key_id: &str, party_id: u16, encrypted_share: &[u8]) -> Result<()>;

    /// Retrieve an encrypted key share.
    fn load_share(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>>;

    /// Delete a key share (for key rotation).
    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()>;

    /// List all key IDs stored in this HSM.
    fn list_keys(&self) -> Result<Vec<String>>;
}
