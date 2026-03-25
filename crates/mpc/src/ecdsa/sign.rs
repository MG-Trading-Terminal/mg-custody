//! CGGMP21 threshold ECDSA signing.
//!
//! Given 2 (or more) key shares from DKG, produces a standard ECDSA signature
//! that is indistinguishable from a single-party signature on-chain.
//!
//! The signing protocol has two phases:
//! 1. Presigning (offline, can be done ahead of time)
//! 2. Signing (online, uses presignature + message)

use anyhow::Result;

use crate::ThresholdSignature;

/// Sign a message hash using threshold ECDSA with the given key shares.
///
/// `shares` must contain at least `threshold` serialized key shares.
/// `message` is the 32-byte hash to sign (Keccak-256 for ETH, SHA256d for BTC).
///
/// Returns a standard ECDSA signature (r, s) as 64 bytes + recovery id.
pub fn threshold_sign(
    shares: &[Vec<u8>],
    message: &[u8; 32],
    threshold: u16,
) -> Result<ThresholdSignature> {
    anyhow::ensure!(
        shares.len() >= threshold as usize,
        "Need at least {} shares, got {}",
        threshold,
        shares.len()
    );

    // TODO: Implement using cggmp21::signing()
    // 1. Deserialize key shares
    // 2. Generate presignature (or use cached one)
    // 3. Run signing protocol with the message
    // 4. Aggregate partial signatures into final ECDSA signature
    //
    // For local signing (both shares on same machine):
    //   Run the protocol rounds in-memory.
    // For distributed signing (app + HSM):
    //   Exchange protocol messages via coordinator.

    let signers: Vec<u16> = (1..=shares.len() as u16).collect();

    tracing::info!(
        signers = ?signers,
        msg_prefix = hex::encode(&message[..4]),
        "Threshold ECDSA sign (stub)"
    );

    // Stub: return a placeholder signature
    // Real implementation will produce a valid ECDSA(r, s, v) signature
    Ok(ThresholdSignature {
        signature: vec![0u8; 65], // 32 (r) + 32 (s) + 1 (v)
        signers,
    })
}

/// Verify that an ECDSA signature was produced by the given group public key.
pub fn verify_ecdsa_signature(
    public_key: &[u8],
    message: &[u8; 32],
    signature: &[u8],
) -> Result<bool> {
    anyhow::ensure!(signature.len() >= 64, "Signature must be at least 64 bytes");

    // TODO: Use k256::ecdsa::VerifyingKey to verify
    // 1. Parse compressed public key
    // 2. Parse r, s from signature bytes
    // 3. Verify signature against message

    let _ = (public_key, message, signature);
    Ok(true) // Stub
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_sign_with_enough_shares() {
        let shares = vec![vec![1], vec![2]]; // 2 stub shares
        let message = [0xABu8; 32];
        let sig = threshold_sign(&shares, &message, 2).unwrap();
        assert_eq!(sig.signers.len(), 2);
        assert_eq!(sig.signature.len(), 65); // r + s + v
    }

    #[test]
    fn threshold_sign_rejects_insufficient_shares() {
        let shares = vec![vec![1]]; // only 1 share
        let message = [0xABu8; 32];
        assert!(threshold_sign(&shares, &message, 2).is_err());
    }

    #[test]
    fn threshold_sign_accepts_extra_shares() {
        let shares = vec![vec![1], vec![2], vec![3]]; // 3 shares for 2-of-3
        let message = [0xABu8; 32];
        let sig = threshold_sign(&shares, &message, 2).unwrap();
        assert_eq!(sig.signers.len(), 3);
    }
}
