//! CGGMP21 threshold ECDSA signing.
//!
//! REAL implementation using round_based::sim::async_env for local MPC simulation.

use anyhow::Result;
use crate::ThresholdSignature;
use super::keygen::FullKeyShare;

/// Message type for the CGGMP21 signing protocol.
type SignMsg = cggmp21::signing::msg::Msg<cggmp21::supported_curves::Secp256k1, sha2::Sha256>;

/// Sign a message using CGGMP21 threshold ECDSA (async).
pub async fn threshold_sign_async(
    share_jsons: &[Vec<u8>],
    message: &[u8; 32],
    threshold: u16,
) -> Result<ThresholdSignature> {
    anyhow::ensure!(share_jsons.len() >= threshold as usize, "Need {} shares", threshold);

    let shares: Vec<FullKeyShare> = share_jsons.iter()
        .map(|s| serde_json::from_slice(s).map_err(|e| anyhow::anyhow!("Deserialize: {e}")))
        .collect::<Result<_>>()?;

    let n = shares.len() as u16;
    let party_indices: Vec<u16> = (0..n).collect();
    // message is already a 32-byte hash. Use from_scalar to avoid double-hashing.
    let scalar = generic_ec::Scalar::<cggmp21::supported_curves::Secp256k1>::from_be_bytes_mod_order(message);
    let data = cggmp21::signing::DataToSign::from_scalar(scalar);

    // Clone data for each party (needed because closures move)
    let shares_ref: Vec<_> = shares.iter().collect();

    // Run MPC protocol
    let results = round_based::sim::async_env::run::<SignMsg, _>(n, |i, party| {
        let share = shares_ref[i as usize];
        let pi = party_indices.clone();
        async move {
            let eid = cggmp21::ExecutionId::new(b"mg-custody-sign");
            let mut rng = rand::thread_rng();
            cggmp21::signing(eid, i, &pi, share)
                .set_digest::<sha2::Sha256>()
                .sign(&mut rng, party, data)
                .await
        }
    }).await;

    // Get first successful signature
    let sig = results.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No results"))?
        .map_err(|e| anyhow::anyhow!("Signing failed: {e}"))?;

    // Extract r and s as 32-byte arrays
    // sig.r is NonZero<Scalar> (the x-coordinate), sig.s is NonZero<Scalar>
    let r_bytes = sig.r.to_be_bytes();
    let s_bytes = sig.s.to_be_bytes();

    let mut sig_bytes = Vec::with_capacity(64);
    sig_bytes.extend_from_slice(r_bytes.as_ref());
    sig_bytes.extend_from_slice(s_bytes.as_ref());

    Ok(ThresholdSignature {
        signature: sig_bytes,
        signers: (0..n).collect(),
    })
}

/// Sync wrapper.
pub fn threshold_sign(shares: &[Vec<u8>], message: &[u8; 32], threshold: u16) -> Result<ThresholdSignature> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(threshold_sign_async(shares, message, threshold))
}

/// Verify ECDSA signature against compressed secp256k1 pubkey.
pub fn verify_ecdsa_signature(pubkey: &[u8], message: &[u8; 32], sig: &[u8]) -> Result<bool> {
    use k256::ecdsa::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey};
    anyhow::ensure!(pubkey.len() == 33 && sig.len() >= 64);
    let vk = VerifyingKey::from_sec1_bytes(pubkey).map_err(|e| anyhow::anyhow!("{e}"))?;
    let s = Signature::from_scalars(
        <[u8;32]>::try_from(&sig[..32]).unwrap(),
        <[u8;32]>::try_from(&sig[32..64]).unwrap(),
    ).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(vk.verify_prehash(message, &s).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_insufficient_shares() {
        assert!(threshold_sign(&[vec![1]], &[0u8; 32], 2).is_err());
    }

    // Full test: keygen + sign + verify. ~3 minutes due to Paillier keygen.
    // Run: cargo test -p custody-mpc -- --ignored real_threshold
    #[tokio::test]
    #[ignore]
    async fn real_threshold_sign_and_verify() {
        use sha2::Digest;
        let (gk, shares) = crate::ecdsa::trusted_dealer_keygen(2, 3).unwrap();
        let msg = sha2::Sha256::digest(b"REAL CGGMP21 MPC!");
        let mut message = [0u8; 32];
        message.copy_from_slice(&msg);

        let sig = threshold_sign_async(&shares[..2], &message, 2).await.unwrap();
        assert_eq!(sig.signature.len(), 64, "sig len = {}", sig.signature.len());

        println!("Group pubkey: {}", hex::encode(&gk.public_key));
        println!("Signature r: {}", hex::encode(&sig.signature[..32]));
        println!("Signature s: {}", hex::encode(&sig.signature[32..64]));

        let valid = verify_ecdsa_signature(&gk.public_key, &message, &sig.signature).unwrap();
        assert!(valid, "CGGMP21 threshold signature MUST verify!");
    }
}
