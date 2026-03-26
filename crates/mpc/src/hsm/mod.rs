//! HSM adapter for key share storage.
//!
//! Provides a trait for storing MPC key shares in hardware security modules.
//! Implementations: software (dev), PKCS#11 (production), cloud HSM.

pub mod pkcs11;
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

    /// Check if this HSM is operational (not a stub).
    fn is_operational(&self) -> bool { false }

    /// HSM type name for logging/diagnostics.
    fn hsm_type(&self) -> &'static str { "unknown" }
}

/// Runtime guard: panic if HSM mode is required but adapter is not operational.
///
/// Call this during startup when config says `hsm_mode = true`.
pub fn assert_hsm_operational(adapter: &dyn HsmAdapter) {
    if !adapter.is_operational() {
        panic!(
            "FATAL: HSM mode enabled but adapter '{}' is not operational. \
             Cannot start custody service with unimplemented HSM. \
             Either configure a real HSM or set hsm_mode = false.",
            adapter.hsm_type()
        );
    }
}

/// Check HSM health (non-panicking version).
pub fn check_hsm_health(adapter: &dyn HsmAdapter) -> HsmHealthCheck {
    HsmHealthCheck {
        hsm_type: adapter.hsm_type().to_string(),
        is_operational: adapter.is_operational(),
        can_list_keys: adapter.list_keys().is_ok(),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HsmHealthCheck {
    pub hsm_type: String,
    pub is_operational: bool,
    pub can_list_keys: bool,
}
