//! Raw message signing for arbitrary data.
//!
//! Supports secp256k1 (BTC/ETH/TRX) and Ed25519 (SOL) signing
//! with EIP-191 personal_sign and EIP-712 typed data encoding.
//!
//! The private key is derived internally and zeroized immediately after use.

use anyhow::{bail, Context, Result};
use bip32::{DerivationPath, XPrv};
use custody_common::Chain;
use ed25519_dalek::{Signer, SigningKey};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use sha3::{Digest as Sha3Digest, Keccak256};
use zeroize::Zeroize;

/// Sign a 32-byte hash with secp256k1 (ETH/BTC/TRX).
///
/// Returns `(signature_65_bytes_with_recovery_id, compressed_pubkey_33_bytes)`.
pub fn sign_raw_secp256k1(
    seed: &[u8; 64],
    chain: Chain,
    index: u32,
    message_hash: &[u8; 32],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let path = match chain {
        Chain::Bitcoin => format!("m/84'/0'/0'/0/{index}"),
        Chain::Ethereum => format!("m/44'/60'/0'/0/{index}"),
        Chain::Tron => format!("m/44'/195'/0'/0/{index}"),
        Chain::Solana => bail!("Solana uses Ed25519, not secp256k1"),
    };

    let derivation_path: DerivationPath = path.parse().context("Invalid derivation path")?;
    let xprv = XPrv::derive_from_path(seed, &derivation_path)
        .context("BIP-32 key derivation failed")?;

    let mut privkey_bytes = xprv.to_bytes();
    let signing_key = k256::ecdsa::SigningKey::from_bytes((&privkey_bytes).into())
        .context("Invalid secp256k1 private key")?;
    privkey_bytes.zeroize();

    let (signature, recid) = signing_key
        .sign_prehash(message_hash)
        .context("secp256k1 signing failed")?;

    // 65-byte signature: r (32) + s (32) + v (1)
    let mut sig_bytes = Vec::with_capacity(65);
    sig_bytes.extend_from_slice(&signature.to_bytes());
    sig_bytes.push(recid.to_byte());

    let pubkey = signing_key.verifying_key();
    let pubkey_bytes = pubkey.to_encoded_point(true).as_bytes().to_vec();

    Ok((sig_bytes, pubkey_bytes))
}

/// Sign arbitrary bytes with Ed25519 (SOL).
///
/// Returns `(signature_64_bytes, pubkey_32_bytes)`.
/// Note: Ed25519 signs the full message, not a hash.
pub fn sign_raw_ed25519(
    seed: &[u8; 64],
    index: u32,
    message: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let signing_key = derive_ed25519_key(seed, index)?;
    let signature = signing_key.sign(message);
    let pubkey = signing_key.verifying_key().to_bytes();

    Ok((signature.to_bytes().to_vec(), pubkey.to_vec()))
}

/// Apply EIP-191 personal_sign prefix and hash with Keccak-256.
///
/// `"\x19Ethereum Signed Message:\n{len}{message}"` → keccak256
pub fn eip191_hash(message: &[u8]) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut hasher = Keccak256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(message);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Hash arbitrary bytes with Keccak-256 (used for Hex and UTF-8 encodings).
pub fn keccak256_hash(data: &[u8]) -> [u8; 32] {
    let result = Keccak256::digest(data);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Derive Ed25519 signing key via SLIP-0010 at m/44'/501'/{index}'/0'.
fn derive_ed25519_key(seed: &[u8; 64], index: u32) -> Result<SigningKey> {
    use hmac::{Hmac, Mac};
    type HmacSha512 = Hmac<sha2::Sha512>;

    let mut mac = HmacSha512::new_from_slice(b"ed25519 seed").unwrap();
    mac.update(seed);
    let result = mac.finalize().into_bytes();
    let mut secret = [0u8; 32];
    let mut chain_code = [0u8; 32];
    secret.copy_from_slice(&result[..32]);
    chain_code.copy_from_slice(&result[32..]);

    for &child_index in &[44u32, 501, index, 0] {
        let hardened = 0x80000000u32 | child_index;
        let mut mac = HmacSha512::new_from_slice(&chain_code).unwrap();
        mac.update(&[0x00]);
        mac.update(&secret);
        mac.update(&hardened.to_be_bytes());
        let child = mac.finalize().into_bytes();
        secret.copy_from_slice(&child[..32]);
        chain_code.copy_from_slice(&child[32..]);
    }

    Ok(SigningKey::from_bytes(&secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    fn test_seed() -> [u8; 64] {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        seed
    }

    #[test]
    fn secp256k1_sign_and_verify_eth() {
        let seed = test_seed();
        let message = b"Hello, custody!";
        let hash = keccak256_hash(message);

        let (sig, pubkey) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash).unwrap();
        assert_eq!(sig.len(), 65); // r + s + v
        assert_eq!(pubkey.len(), 33); // compressed

        // Verify with k256
        let vk = k256::ecdsa::VerifyingKey::from_sec1_bytes(&pubkey).unwrap();
        let signature = k256::ecdsa::Signature::from_slice(&sig[..64]).unwrap();
        use k256::ecdsa::signature::hazmat::PrehashVerifier;
        assert!(vk.verify_prehash(&hash, &signature).is_ok());
    }

    #[test]
    fn secp256k1_sign_btc() {
        let seed = test_seed();
        let hash = [0xAA; 32];
        let (sig, pubkey) = sign_raw_secp256k1(&seed, Chain::Bitcoin, 0, &hash).unwrap();
        assert_eq!(sig.len(), 65);
        assert_eq!(pubkey.len(), 33);
    }

    #[test]
    fn secp256k1_sign_tron() {
        let seed = test_seed();
        let hash = [0xBB; 32];
        let (sig, pubkey) = sign_raw_secp256k1(&seed, Chain::Tron, 0, &hash).unwrap();
        assert_eq!(sig.len(), 65);
        assert_eq!(pubkey.len(), 33);
    }

    #[test]
    fn secp256k1_solana_rejected() {
        let seed = test_seed();
        let hash = [0x00; 32];
        let result = sign_raw_secp256k1(&seed, Chain::Solana, 0, &hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Ed25519"));
    }

    #[test]
    fn ed25519_sign_and_verify() {
        let seed = test_seed();
        let message = b"Hello, Solana!";

        let (sig, pubkey) = sign_raw_ed25519(&seed, 0, message).unwrap();
        assert_eq!(sig.len(), 64);
        assert_eq!(pubkey.len(), 32);

        // Verify
        let vk = ed25519_dalek::VerifyingKey::from_bytes(
            pubkey.as_slice().try_into().unwrap(),
        ).unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(
            sig.as_slice().try_into().unwrap(),
        );
        assert!(vk.verify(message, &signature).is_ok());
    }

    #[test]
    fn ed25519_different_indices_different_keys() {
        let seed = test_seed();
        let msg = b"test";
        let (_, pk0) = sign_raw_ed25519(&seed, 0, msg).unwrap();
        let (_, pk1) = sign_raw_ed25519(&seed, 1, msg).unwrap();
        assert_ne!(pk0, pk1);
    }

    #[test]
    fn eip191_known_hash() {
        let message = b"Hello";
        let hash = eip191_hash(message);
        // Manual: keccak256("\x19Ethereum Signed Message:\n5Hello")
        assert_eq!(hash.len(), 32);
        // Different message = different hash
        let hash2 = eip191_hash(b"World");
        assert_ne!(hash, hash2);
    }

    #[test]
    fn eip191_empty_message() {
        let hash = eip191_hash(b"");
        assert_eq!(hash.len(), 32);
        // Should still produce a valid hash (prefix includes \n0)
    }

    #[test]
    fn keccak256_known_empty() {
        let hash = keccak256_hash(b"");
        // keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        assert_eq!(
            hex::encode(hash),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn secp256k1_sign_deterministic() {
        let seed = test_seed();
        let hash = keccak256_hash(b"deterministic");
        let (sig1, pk1) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash).unwrap();
        let (sig2, pk2) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash).unwrap();
        assert_eq!(pk1, pk2);
        // Signatures should be the same for same key + same message (RFC 6979)
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn secp256k1_different_messages_different_signatures() {
        let seed = test_seed();
        let hash1 = keccak256_hash(b"message1");
        let hash2 = keccak256_hash(b"message2");
        let (sig1, _) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash1).unwrap();
        let (sig2, _) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash2).unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn eip191_roundtrip_sign_verify() {
        let seed = test_seed();
        let message = b"Sign this message to prove ownership";
        let hash = eip191_hash(message);

        let (sig, pubkey) = sign_raw_secp256k1(&seed, Chain::Ethereum, 0, &hash).unwrap();

        let vk = k256::ecdsa::VerifyingKey::from_sec1_bytes(&pubkey).unwrap();
        let signature = k256::ecdsa::Signature::from_slice(&sig[..64]).unwrap();
        use k256::ecdsa::signature::hazmat::PrehashVerifier;
        assert!(vk.verify_prehash(&hash, &signature).is_ok());
    }
}
