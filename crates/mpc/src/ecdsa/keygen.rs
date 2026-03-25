//! CGGMP21 distributed key generation for threshold ECDSA.
//!
//! Generates key shares for 2-of-3 (or arbitrary t-of-n) threshold ECDSA.
//! Each party receives a share; any t shares can produce a valid signature.
//!
//! Protocol: CGGMP'24 (improved CGGMP21) via the `cggmp21` crate.
//! Audited by Kudelski Security.

use anyhow::{Context, Result};
use uuid::Uuid;

use super::types::EcdsaGroupKey;

/// Run a simulated local DKG for development/testing.
///
/// In production, DKG messages are exchanged between physically separate parties.
/// This function runs all parties in the same process for testing.
///
/// Returns the group public key and party key shares (as serialized bytes).
pub fn local_dkg(threshold: u16, parties: u16) -> Result<(EcdsaGroupKey, Vec<Vec<u8>>)> {
    anyhow::ensure!(threshold >= 2, "Threshold must be at least 2");
    anyhow::ensure!(parties >= threshold, "Parties must be >= threshold");
    anyhow::ensure!(parties <= 10, "Max 10 parties supported");

    let key_id = Uuid::new_v4().to_string();

    // TODO: Implement using cggmp21::keygen()
    // The cggmp21 API requires:
    // 1. Each party runs aux_info_gen() to generate Paillier keys
    // 2. Parties exchange messages in rounds
    // 3. Each party runs keygen() to get their key share
    // 4. Group public key is derived from the shares
    //
    // For local DKG, we simulate message passing in-memory.
    // For production, messages go through the coordinator service.

    tracing::info!(
        key_id = %key_id,
        threshold,
        parties,
        "DKG ceremony initiated (stub)"
    );

    // Stub: return placeholder data
    // Real implementation will use cggmp21::keygen protocol
    let group_key = EcdsaGroupKey {
        key_id: key_id.clone(),
        public_key: vec![0x02; 33], // placeholder compressed pubkey
        threshold,
        parties,
    };

    let shares: Vec<Vec<u8>> = (0..parties)
        .map(|i| {
            // Placeholder: each "share" is just a tagged empty vec
            serde_json::to_vec(&serde_json::json!({
                "party_index": i + 1,
                "key_id": key_id,
                "stub": true,
            }))
            .unwrap()
        })
        .collect();

    Ok((group_key, shares))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dkg_2_of_3() {
        let (group_key, shares) = local_dkg(2, 3).unwrap();
        assert_eq!(group_key.threshold, 2);
        assert_eq!(group_key.parties, 3);
        assert_eq!(shares.len(), 3);
        assert!(!group_key.key_id.is_empty());
    }

    #[test]
    fn local_dkg_3_of_5() {
        let (group_key, shares) = local_dkg(3, 5).unwrap();
        assert_eq!(shares.len(), 5);
        assert_eq!(group_key.threshold, 3);
    }

    #[test]
    fn local_dkg_rejects_invalid_params() {
        assert!(local_dkg(1, 3).is_err()); // threshold < 2
        assert!(local_dkg(5, 3).is_err()); // parties < threshold
    }

    #[test]
    fn each_share_is_distinct() {
        let (_, shares) = local_dkg(2, 3).unwrap();
        assert_ne!(shares[0], shares[1]);
        assert_ne!(shares[1], shares[2]);
    }
}
