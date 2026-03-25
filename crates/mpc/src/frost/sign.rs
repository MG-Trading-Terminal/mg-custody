//! FROST threshold Ed25519 signing.
//!
//! Two-round signing protocol:
//! Round 1: Each signer generates nonce commitments
//! Round 2: Each signer produces a signature share
//! Aggregation: Combine shares into final Ed25519 signature

use anyhow::{Context, Result};
use frost_ed25519 as frost;
use std::collections::BTreeMap;

use crate::ThresholdSignature;

/// Sign a message using FROST threshold Ed25519.
///
/// `key_packages_json`: serialized KeyPackage for each participating signer.
/// `pubkey_package_json`: serialized PublicKeyPackage (group verification key).
/// `message`: the raw message bytes to sign.
///
/// Returns a standard Ed25519 signature (64 bytes).
pub fn frost_sign(
    key_packages_json: &[(u16, &[u8])],
    pubkey_package: &frost::keys::PublicKeyPackage,
    message: &[u8],
    threshold: u16,
) -> Result<ThresholdSignature> {
    anyhow::ensure!(
        key_packages_json.len() >= threshold as usize,
        "Need at least {} signers, got {}",
        threshold,
        key_packages_json.len()
    );

    let mut rng = rand::thread_rng();

    // Deserialize key packages
    let key_packages: BTreeMap<frost::Identifier, frost::keys::KeyPackage> =
        key_packages_json
            .iter()
            .map(|(_, data)| {
                let kp: frost::keys::KeyPackage =
                    serde_json::from_slice(data).context("Failed to deserialize KeyPackage")?;
                Ok((*kp.identifier(), kp))
            })
            .collect::<Result<_>>()?;

    // Round 1: Generate nonces and commitments
    let mut nonces_map = BTreeMap::new();
    let mut commitments_map = BTreeMap::new();

    for (id, key_package) in &key_packages {
        let (nonces, commitments) =
            frost::round1::commit(key_package.signing_share(), &mut rng);
        nonces_map.insert(*id, nonces);
        commitments_map.insert(*id, commitments);
    }

    // Build signing package
    let signing_package = frost::SigningPackage::new(commitments_map, message);

    // Round 2: Each signer produces a signature share
    let mut signature_shares = BTreeMap::new();

    for (id, key_package) in &key_packages {
        let nonces = nonces_map
            .get(id)
            .context("Missing nonces for signer")?;
        let share = frost::round2::sign(&signing_package, nonces, key_package)
            .context("FROST round 2 signing failed")?;
        signature_shares.insert(*id, share);
    }

    // Aggregate signature shares
    let group_signature =
        frost::aggregate(&signing_package, &signature_shares, pubkey_package)
            .context("FROST signature aggregation failed")?;

    let sig_bytes = group_signature.serialize()?;
    let signers: Vec<u16> = key_packages_json.iter().map(|(id, _)| *id).collect();

    Ok(ThresholdSignature {
        signature: sig_bytes.to_vec(),
        signers,
    })
}

/// Verify a FROST Ed25519 signature against the group public key.
pub fn verify_frost_signature(
    group_pubkey_bytes: &[u8; 32],
    message: &[u8],
    signature_bytes: &[u8; 64],
) -> Result<bool> {
    let vk = ed25519_dalek::VerifyingKey::from_bytes(group_pubkey_bytes)
        .context("Invalid Ed25519 public key")?;
    let sig = ed25519_dalek::Signature::from_bytes(signature_bytes);
    Ok(vk.verify_strict(message, &sig).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_frost_2_of_3() -> (
        frost::keys::PublicKeyPackage,
        Vec<(frost::Identifier, frost::keys::KeyPackage)>,
    ) {
        let mut rng = rand::thread_rng();
        let (shares, pubkey_package) = frost::keys::generate_with_dealer(
            3,
            2,
            frost::keys::IdentifierList::Default,
            &mut rng,
        )
        .unwrap();

        let key_packages: Vec<(frost::Identifier, frost::keys::KeyPackage)> = shares
            .into_iter()
            .map(|(id, share)| {
                let kp = frost::keys::KeyPackage::try_from(share).unwrap();
                (id, kp)
            })
            .collect();

        (pubkey_package, key_packages)
    }

    #[test]
    fn frost_sign_2_of_3() {
        let (pubkey_package, key_packages) = setup_frost_2_of_3();
        let message = b"test message for FROST signing";

        // Use first 2 of 3 signers
        let signers: Vec<(u16, Vec<u8>)> = key_packages[..2]
            .iter()
            .enumerate()
            .map(|(i, (_, kp))| {
                ((i + 1) as u16, serde_json::to_vec(kp).unwrap())
            })
            .collect();

        let signer_refs: Vec<(u16, &[u8])> = signers.iter().map(|(id, d)| (*id, d.as_slice())).collect();

        let result = frost_sign(&signer_refs, &pubkey_package, message, 2);
        assert!(result.is_ok(), "FROST signing should succeed: {:?}", result.err());

        let sig = result.unwrap();
        assert_eq!(sig.signature.len(), 64); // Ed25519 signature
        assert_eq!(sig.signers.len(), 2);
    }

    #[test]
    fn frost_sign_and_verify() {
        let (pubkey_package, key_packages) = setup_frost_2_of_3();
        let message = b"verify this FROST signature";

        let signers: Vec<(u16, Vec<u8>)> = key_packages[..2]
            .iter()
            .enumerate()
            .map(|(i, (_, kp))| {
                ((i + 1) as u16, serde_json::to_vec(kp).unwrap())
            })
            .collect();

        let signer_refs: Vec<(u16, &[u8])> = signers.iter().map(|(id, d)| (*id, d.as_slice())).collect();

        let sig = frost_sign(&signer_refs, &pubkey_package, message, 2).unwrap();

        // Verify with group public key
        let group_pubkey_vec = pubkey_package.verifying_key().serialize().unwrap();
        let group_pubkey: [u8; 32] = group_pubkey_vec.as_slice().try_into().unwrap();
        let sig_bytes: [u8; 64] = sig.signature.as_slice().try_into().unwrap();
        let valid = verify_frost_signature(&group_pubkey,message, &sig_bytes).unwrap();
        assert!(valid, "FROST signature should verify against group public key");
    }

    #[test]
    fn frost_sign_different_signer_combinations() {
        let (pubkey_package, key_packages) = setup_frost_2_of_3();
        let message = b"any 2-of-3 should work";

        // Try all 3 combinations of 2-of-3
        let combos = [[0, 1], [0, 2], [1, 2]];

        for combo in &combos {
            let signers: Vec<(u16, Vec<u8>)> = combo
                .iter()
                .enumerate()
                .map(|(i, &idx)| {
                    let (_, kp) = &key_packages[idx];
                    ((i + 1) as u16, serde_json::to_vec(kp).unwrap())
                })
                .collect();

            let signer_refs: Vec<(u16, &[u8])> = signers.iter().map(|(id, d)| (*id, d.as_slice())).collect();

            let sig = frost_sign(&signer_refs, &pubkey_package, message, 2).unwrap();

            let gpk_vec = pubkey_package.verifying_key().serialize().unwrap();
            let gpk: [u8; 32] = gpk_vec.as_slice().try_into().unwrap();
            let sig_bytes: [u8; 64] = sig.signature.as_slice().try_into().unwrap();
            let valid = verify_frost_signature(&gpk, message, &sig_bytes).unwrap();
            assert!(valid, "Combination {:?} should produce valid signature", combo);
        }
    }

    #[test]
    fn frost_sign_wrong_message_fails_verify() {
        let (pubkey_package, key_packages) = setup_frost_2_of_3();

        let signers: Vec<(u16, Vec<u8>)> = key_packages[..2]
            .iter()
            .enumerate()
            .map(|(i, (_, kp))| {
                ((i + 1) as u16, serde_json::to_vec(kp).unwrap())
            })
            .collect();

        let signer_refs: Vec<(u16, &[u8])> = signers.iter().map(|(id, d)| (*id, d.as_slice())).collect();

        let sig = frost_sign(&signer_refs, &pubkey_package, b"original message", 2).unwrap();

        let group_pubkey_vec = pubkey_package.verifying_key().serialize().unwrap();
        let group_pubkey: [u8; 32] = group_pubkey_vec.as_slice().try_into().unwrap();
        let sig_bytes: [u8; 64] = sig.signature.as_slice().try_into().unwrap();
        let valid = verify_frost_signature(&group_pubkey,b"different message", &sig_bytes).unwrap();
        assert!(!valid, "Signature should not verify against different message");
    }
}
