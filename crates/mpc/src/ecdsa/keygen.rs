//! CGGMP21 distributed key generation for threshold ECDSA.
//!
//! Uses the `cggmp21` crate's trusted dealer for key share generation.
//! Produces REAL key shares for secp256k1 threshold signing.

use anyhow::{Context, Result};
use generic_ec::Point;
use uuid::Uuid;

use super::types::EcdsaGroupKey;

/// Generate ECDSA key shares using CGGMP21 trusted dealer.
///
/// Returns the group public key and serialized core key shares.
pub fn trusted_dealer_keygen(threshold: u16, parties: u16) -> Result<(EcdsaGroupKey, Vec<Vec<u8>>)> {
    anyhow::ensure!(threshold >= 2, "Threshold must be at least 2");
    anyhow::ensure!(parties >= threshold, "Parties must be >= threshold");

    let key_id = Uuid::new_v4().to_string();
    let mut rng = rand::thread_rng();

    // Generate core key shares
    let shares = cggmp21::trusted_dealer::builder::<
        cggmp21::supported_curves::Secp256k1,
        cggmp21::security_level::SecurityLevel128,
    >(parties)
        .set_threshold(Some(threshold))
        .generate_core_shares(&mut rng)
        .map_err(|e| anyhow::anyhow!("CGGMP21 keygen failed: {e:?}"))?;

    // Extract compressed public key from first share
    let shared_pk = shares[0].shared_public_key;
    let pk_point: Point<cggmp21::supported_curves::Secp256k1> = *shared_pk;
    let pk_bytes = pk_point.to_bytes(true).as_ref().to_vec();

    // Serialize each share
    let serialized_shares: Vec<Vec<u8>> = shares
        .iter()
        .enumerate()
        .map(|(i, share)| {
            serde_json::to_vec(share)
                .with_context(|| format!("Failed to serialize share {i}"))
        })
        .collect::<Result<_>>()?;

    let group_key = EcdsaGroupKey {
        key_id: key_id.clone(),
        public_key: pk_bytes,
        threshold,
        parties,
    };

    tracing::info!(
        key_id = %key_id,
        threshold,
        parties,
        pubkey_len = group_key.public_key.len(),
        "CGGMP21 keygen complete"
    );

    Ok((group_key, serialized_shares))
}

/// Update the mod.rs export
pub use super::types::EcdsaGroupKey as GroupKey;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_cggmp21_keygen_2_of_3() {
        let (group_key, shares) = trusted_dealer_keygen(2, 3).unwrap();
        assert_eq!(group_key.threshold, 2);
        assert_eq!(group_key.parties, 3);
        assert_eq!(shares.len(), 3);
        assert!(!group_key.key_id.is_empty());
        // Compressed secp256k1 public key = 33 bytes
        assert_eq!(group_key.public_key.len(), 33, "Expected 33-byte compressed pubkey, got {}", group_key.public_key.len());
        assert!(group_key.public_key[0] == 0x02 || group_key.public_key[0] == 0x03,
            "First byte should be 02 or 03 for compressed key, got 0x{:02x}", group_key.public_key[0]);
    }

    #[test]
    fn keygen_shares_are_substantial() {
        let (_, shares) = trusted_dealer_keygen(2, 3).unwrap();
        for (i, share) in shares.iter().enumerate() {
            assert!(share.len() > 50, "Share {i} too small: {} bytes", share.len());
        }
    }

    #[test]
    fn keygen_rejects_invalid() {
        assert!(trusted_dealer_keygen(1, 3).is_err());
        assert!(trusted_dealer_keygen(5, 3).is_err());
    }

    #[test]
    fn each_share_distinct() {
        let (_, shares) = trusted_dealer_keygen(2, 3).unwrap();
        assert_ne!(shares[0], shares[1]);
        assert_ne!(shares[1], shares[2]);
    }

    #[test]
    fn different_ceremonies_different_keys() {
        let (gk1, _) = trusted_dealer_keygen(2, 3).unwrap();
        let (gk2, _) = trusted_dealer_keygen(2, 3).unwrap();
        assert_ne!(gk1.public_key, gk2.public_key);
    }
}
