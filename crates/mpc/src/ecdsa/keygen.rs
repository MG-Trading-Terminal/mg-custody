//! CGGMP21 distributed key generation for threshold ECDSA.
//!
//! Uses `generate_shares()` (NOT `generate_core_shares()`) to produce
//! FULL KeyShare objects that include AuxInfo (Paillier keys).
//! These can be directly used for threshold signing.

use anyhow::{Context, Result};
use generic_ec::Point;
use uuid::Uuid;

use super::types::EcdsaGroupKey;

/// Type alias for a full CGGMP21 KeyShare (CoreKeyShare + AuxInfo, validated).
pub type FullKeyShare = cggmp21::key_share::KeyShare<
    cggmp21::supported_curves::Secp256k1,
    cggmp21::security_level::SecurityLevel128,
>;

/// Generate FULL ECDSA key shares using CGGMP21 trusted dealer.
///
/// Returns group public key + serialized FULL key shares.
/// These shares contain Paillier keys and can be used directly for signing.
pub fn trusted_dealer_keygen(threshold: u16, parties: u16) -> Result<(EcdsaGroupKey, Vec<Vec<u8>>)> {
    anyhow::ensure!(threshold >= 2, "Threshold must be at least 2");
    anyhow::ensure!(parties >= threshold, "Parties must be >= threshold");

    let key_id = Uuid::new_v4().to_string();
    let mut rng = rand::thread_rng();

    // generate_shares() = CoreKeyShare + AuxInfo (Paillier) + validated
    let shares: Vec<FullKeyShare> = cggmp21::trusted_dealer::builder::<
        cggmp21::supported_curves::Secp256k1,
        cggmp21::security_level::SecurityLevel128,
    >(parties)
        .set_threshold(Some(threshold))
        .generate_shares(&mut rng)
        .map_err(|e| anyhow::anyhow!("CGGMP21 keygen failed: {e:?}"))?;

    // Extract compressed public key
    let shared_pk = shares[0].shared_public_key;
    let pk_point: Point<cggmp21::supported_curves::Secp256k1> = *shared_pk;
    let pk_bytes = pk_point.to_bytes(true).as_ref().to_vec();

    // Serialize full key shares
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
        key_id = %key_id, threshold, parties,
        pubkey_len = group_key.public_key.len(),
        "CGGMP21 full keygen complete (with AuxInfo)"
    );

    Ok((group_key, serialized_shares))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_keygen_2_of_3() {
        let (gk, shares) = trusted_dealer_keygen(2, 3).unwrap();
        assert_eq!(gk.threshold, 2);
        assert_eq!(gk.parties, 3);
        assert_eq!(shares.len(), 3);
        assert_eq!(gk.public_key.len(), 33);
        assert!(gk.public_key[0] == 0x02 || gk.public_key[0] == 0x03);

        // Full shares include Paillier keys — should be large
        for (i, share) in shares.iter().enumerate() {
            assert!(share.len() > 1000, "Share {i}: {} bytes (expected >1KB)", share.len());
        }
    }

    #[test]
    fn shares_deserialize_as_full_keyshare() {
        let (_, shares) = trusted_dealer_keygen(2, 3).unwrap();
        for (i, s) in shares.iter().enumerate() {
            let parsed: Result<FullKeyShare, _> = serde_json::from_slice(s);
            assert!(parsed.is_ok(), "Share {i} deserialize failed: {:?}", parsed.err());
        }
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(trusted_dealer_keygen(1, 3).is_err());
        assert!(trusted_dealer_keygen(5, 3).is_err());
    }

    #[test]
    fn different_keys_per_ceremony() {
        let (gk1, _) = trusted_dealer_keygen(2, 3).unwrap();
        let (gk2, _) = trusted_dealer_keygen(2, 3).unwrap();
        assert_ne!(gk1.public_key, gk2.public_key);
    }
}
