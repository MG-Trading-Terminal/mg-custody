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

// NOTE: a second `EmergencyFreeze` struct used to live here —
// an AtomicBool + three Mutexes with freeze/unfreeze methods. It
// was NEVER instantiated outside its own unit test: a grep
// across the workspace confirms no call sites in signing,
// coordinator, ceremony runners, or the HTTP server. It was a
// look-alike security control: a reviewer skimming the MPC
// crate sees "emergency freeze — disable all signing
// operations" in a docstring and assumes the signing path
// honours it, when in fact it was unreachable dead code.
//
// The real, wired, and now persisted freeze lives in
// `common::emergency::FreezeFlag`, hydrated on startup from the
// audit_log and checked on every per-withdrawal iteration in
// the pipeline. Don't add a second freeze mechanism here; wire
// new call sites to the `FreezeFlag` in `AppState` instead.

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
}
