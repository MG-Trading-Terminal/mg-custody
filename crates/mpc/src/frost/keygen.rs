//! FROST distributed key generation for threshold Ed25519.
//!
//! Uses the `frost-ed25519` crate (IETF RFC 9591, audited by NCC Group).
//! Supports both trusted dealer keygen and distributed DKG.

use anyhow::{Context, Result};
use frost_ed25519 as frost;
use uuid::Uuid;

use super::types::FrostGroupKey;

/// Run trusted dealer key generation for FROST Ed25519.
///
/// A trusted dealer generates all shares centrally. This is simpler but requires
/// trusting the dealer. For production, use DKG instead.
///
/// Returns the group verifying key and serialized key packages for each party.
pub fn trusted_dealer_keygen(
    threshold: u16,
    parties: u16,
) -> Result<(FrostGroupKey, Vec<(u16, Vec<u8>)>)> {
    anyhow::ensure!(threshold >= 2, "Threshold must be at least 2");
    anyhow::ensure!(parties >= threshold, "Parties must be >= threshold");

    let key_id = Uuid::new_v4().to_string();
    // OsRng for the group secret. This RNG produces the FROST
    // group signing key — the threshold property only holds if
    // the key is uniformly random. `thread_rng()` is
    // cryptographically secure in current `rand`, but `OsRng`
    // makes the kernel-entropy path explicit and matches the
    // BIP-39 / Shamir / seed-tool convention elsewhere in the
    // tree. Bias here = weak threshold key = private key
    // recovery given enough signatures.
    let mut rng = rand::rngs::OsRng;

    let (shares, pubkey_package) = frost::keys::generate_with_dealer(
        parties,
        threshold,
        frost::keys::IdentifierList::Default,
        &mut rng,
    )
    .context("FROST trusted dealer keygen failed")?;

    // Extract group verifying key
    let group_pubkey = pubkey_package.verifying_key();
    let group_pubkey_bytes = group_pubkey.serialize()
        .map_err(|e| anyhow::anyhow!("Failed to serialize group public key: {e}"))?;

    // Serialize each key package
    let key_packages: Vec<(u16, Vec<u8>)> = shares
        .into_iter()
        .map(|(id, secret_share)| {
            let key_package = frost::keys::KeyPackage::try_from(secret_share)
                .expect("valid secret share");
            let serialized = serde_json::to_vec(&key_package)
                .expect("KeyPackage serialization");
            // Convert Identifier to u16 for storage
            let id_bytes: [u8; 2] = id.serialize()[..2].try_into().unwrap_or([0, 0]);
            let party_id = u16::from_be_bytes(id_bytes);
            (party_id, serialized)
        })
        .collect();

    let group_key = FrostGroupKey {
        key_id,
        public_key: group_pubkey_bytes.to_vec(),
        threshold,
        parties,
    };

    tracing::info!(
        key_id = %group_key.key_id,
        threshold,
        parties,
        "FROST trusted dealer keygen complete"
    );

    Ok((group_key, key_packages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frost_keygen_2_of_3() {
        let (group_key, packages) = trusted_dealer_keygen(2, 3).unwrap();
        assert_eq!(group_key.threshold, 2);
        assert_eq!(group_key.parties, 3);
        assert_eq!(packages.len(), 3);
        assert_eq!(group_key.public_key.len(), 32);
        assert!(!group_key.key_id.is_empty());
    }

    #[test]
    fn frost_keygen_3_of_5() {
        let (group_key, packages) = trusted_dealer_keygen(3, 5).unwrap();
        assert_eq!(packages.len(), 5);
        assert_eq!(group_key.threshold, 3);
    }

    #[test]
    fn frost_keygen_produces_valid_ed25519_pubkey() {
        let (group_key, _) = trusted_dealer_keygen(2, 3).unwrap();
        // Should be a valid 32-byte Ed25519 public key
        assert_eq!(group_key.public_key.len(), 32);
        // Verify it can be parsed as an Ed25519 verifying key
        let vk = ed25519_dalek::VerifyingKey::from_bytes(
            group_key.public_key.as_slice().try_into().unwrap(),
        );
        assert!(vk.is_ok(), "Group public key should be a valid Ed25519 key");
    }

    #[test]
    fn frost_keygen_different_runs_produce_different_keys() {
        let (gk1, _) = trusted_dealer_keygen(2, 3).unwrap();
        let (gk2, _) = trusted_dealer_keygen(2, 3).unwrap();
        assert_ne!(gk1.public_key, gk2.public_key);
        assert_ne!(gk1.key_id, gk2.key_id);
    }

    #[test]
    fn frost_keygen_rejects_invalid_params() {
        assert!(trusted_dealer_keygen(1, 3).is_err());
        assert!(trusted_dealer_keygen(5, 3).is_err());
    }
}
