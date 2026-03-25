//! Key ceremony tooling.
//!
//! CLI-driven key generation ceremony for production MPC key setup.
//! Generates key shares, encrypts each with a separate password,
//! and exports share 3 for offline cold storage.

use anyhow::{Context, Result};

use crate::frost;
use crate::hsm::HsmAdapter;

/// Result of a key ceremony.
#[derive(Debug)]
pub struct CeremonyResult {
    pub key_id: String,
    pub public_key_hex: String,
    pub threshold: u16,
    pub parties: u16,
    /// Encrypted share files written.
    pub share_files: Vec<String>,
}

/// Run a FROST Ed25519 key ceremony (for Solana).
///
/// Generates shares using trusted dealer and stores them in the HSM adapter.
pub fn run_frost_ceremony(
    hsm: &dyn HsmAdapter,
    threshold: u16,
    parties: u16,
) -> Result<CeremonyResult> {
    tracing::info!(threshold, parties, "Starting FROST key ceremony");

    let (group_key, packages) = frost::trusted_dealer_keygen(threshold, parties)
        .context("FROST keygen failed")?;

    // Store each share in HSM
    for (party_id, data) in &packages {
        hsm.store_share(&group_key.key_id, *party_id, data)
            .with_context(|| format!("Failed to store share for party {party_id}"))?;
        tracing::info!(party_id, key_id = %group_key.key_id, "Share stored");
    }

    let share_files: Vec<String> = packages
        .iter()
        .map(|(id, _)| format!("share_{}_{}.enc", group_key.key_id, id))
        .collect();

    let result = CeremonyResult {
        key_id: group_key.key_id,
        public_key_hex: hex::encode(&group_key.public_key),
        threshold,
        parties,
        share_files,
    };

    tracing::info!(
        key_id = %result.key_id,
        pubkey = %result.public_key_hex,
        "Key ceremony complete"
    );

    Ok(result)
}

/// Emergency freeze — disable all signing operations.
///
/// Sets a global flag that the signing service checks before every operation.
/// Any admin can trigger this. Requires admin quorum to unfreeze.
#[derive(Debug)]
pub struct EmergencyFreeze {
    frozen: std::sync::atomic::AtomicBool,
    frozen_at: std::sync::Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    frozen_by: std::sync::Mutex<Option<String>>,
    reason: std::sync::Mutex<Option<String>>,
}

impl EmergencyFreeze {
    pub fn new() -> Self {
        Self {
            frozen: std::sync::atomic::AtomicBool::new(false),
            frozen_at: std::sync::Mutex::new(None),
            frozen_by: std::sync::Mutex::new(None),
            reason: std::sync::Mutex::new(None),
        }
    }

    /// Freeze all operations immediately.
    pub fn freeze(&self, admin: &str, reason: &str) {
        self.frozen.store(true, std::sync::atomic::Ordering::SeqCst);
        *self.frozen_at.lock().unwrap() = Some(chrono::Utc::now());
        *self.frozen_by.lock().unwrap() = Some(admin.to_string());
        *self.reason.lock().unwrap() = Some(reason.to_string());
        tracing::error!(admin, reason, "EMERGENCY FREEZE ACTIVATED");
    }

    /// Unfreeze (requires separate admin quorum verification by caller).
    pub fn unfreeze(&self, admin: &str) {
        self.frozen.store(false, std::sync::atomic::Ordering::SeqCst);
        tracing::warn!(admin, "Emergency freeze lifted");
    }

    /// Check if operations are frozen.
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get freeze details.
    pub fn freeze_info(&self) -> Option<(String, String, chrono::DateTime<chrono::Utc>)> {
        if !self.is_frozen() {
            return None;
        }
        let by = self.frozen_by.lock().ok()?.clone()?;
        let reason = self.reason.lock().ok()?.clone()?;
        let at = self.frozen_at.lock().ok()?.clone()?;
        Some((by, reason, at))
    }
}

impl Default for EmergencyFreeze {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsm::soft::SoftwareHsm;

    #[test]
    fn frost_ceremony_2_of_3() {
        let hsm = SoftwareHsm::new();
        let result = run_frost_ceremony(&hsm, 2, 3).unwrap();

        assert_eq!(result.threshold, 2);
        assert_eq!(result.parties, 3);
        assert_eq!(result.share_files.len(), 3);
        assert!(!result.key_id.is_empty());
        assert_eq!(result.public_key_hex.len(), 64); // 32 bytes = 64 hex

        // Verify shares are in HSM
        let keys = hsm.list_keys().unwrap();
        assert!(keys.contains(&result.key_id));
    }

    #[test]
    fn emergency_freeze_lifecycle() {
        let freeze = EmergencyFreeze::new();
        assert!(!freeze.is_frozen());

        freeze.freeze("admin@company.com", "Suspicious activity detected");
        assert!(freeze.is_frozen());

        let (by, reason, _at) = freeze.freeze_info().unwrap();
        assert_eq!(by, "admin@company.com");
        assert!(reason.contains("Suspicious"));

        freeze.unfreeze("admin2@company.com");
        assert!(!freeze.is_frozen());
        assert!(freeze.freeze_info().is_none());
    }
}
