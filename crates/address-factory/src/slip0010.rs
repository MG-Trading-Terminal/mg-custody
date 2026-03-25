//! SLIP-0010 Ed25519 hierarchical deterministic key derivation.
//!
//! Ed25519 does not support BIP-32's non-hardened child derivation.
//! SLIP-0010 defines an alternative scheme using only hardened derivation
//! with HMAC-SHA512.
//!
//! Reference: <https://github.com/satoshilabs/slips/blob/master/slip-0010.md>

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::hd::SeedBytes;

type HmacSha512 = Hmac<Sha512>;

/// Ed25519 key pair derived via SLIP-0010.
///
/// Only the secret key is stored; the public key is derived on demand
/// to minimize the window of exposure.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Ed25519KeyPair {
    secret: [u8; 32],
    chain_code: [u8; 32],
}

impl std::fmt::Debug for Ed25519KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ed25519KeyPair(***)")
    }
}

impl Ed25519KeyPair {
    /// Extract the 32-byte Ed25519 public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&self.secret);
        let verifying_key = signing_key.verifying_key();
        verifying_key.to_bytes()
    }
}

/// Derive an Ed25519 key pair from a seed at the given SLIP-0010 path.
///
/// All path indices are implicitly hardened (Ed25519 SLIP-0010 only supports
/// hardened derivation). Pass raw indices — the hardened bit is added automatically.
///
/// Solana standard path: `[44, 501, account_index, 0]` → m/44'/501'/{index}'/0'
pub fn derive_ed25519_keypair(seed: &SeedBytes, path: &[u32]) -> Result<Ed25519KeyPair> {
    // Step 1: Master key generation
    // HMAC-SHA512(key="ed25519 seed", data=seed)
    let mut mac = HmacSha512::new_from_slice(b"ed25519 seed")
        .context("HMAC-SHA512 initialization failed")?;
    mac.update(&seed.0);
    let mut result = mac.finalize().into_bytes();

    let mut secret = [0u8; 32];
    let mut chain_code = [0u8; 32];
    secret.copy_from_slice(&result[..32]);
    chain_code.copy_from_slice(&result[32..]);
    result.zeroize();

    // Step 2: Child derivation (all hardened)
    for &index in path {
        let hardened_index = 0x80000000u32 | index;

        let mut mac = HmacSha512::new_from_slice(&chain_code)
            .context("HMAC-SHA512 child derivation failed")?;

        // data = 0x00 || key (32 bytes) || ser32(hardened_index)
        mac.update(&[0x00]);
        mac.update(&secret);
        mac.update(&hardened_index.to_be_bytes());

        secret.zeroize();
        chain_code.zeroize();

        let mut child_result = mac.finalize().into_bytes();
        secret.copy_from_slice(&child_result[..32]);
        chain_code.copy_from_slice(&child_result[32..]);
        child_result.zeroize();
    }

    Ok(Ed25519KeyPair { secret, chain_code })
}

/// Build the Solana SLIP-0010 path for a given account index.
///
/// Returns `[44, 501, index, 0]` → m/44'/501'/{index}'/0'
pub fn solana_path(index: u32) -> Vec<u32> {
    vec![44, 501, index, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    // SLIP-0010 test vectors use variable-length seeds (16 bytes),
    // so we test with `derive_from_raw_seed` helper that takes &[u8].

    /// Derive from raw seed bytes (variable length, for test vectors).
    fn derive_from_raw_seed(seed: &[u8], path: &[u32]) -> Ed25519KeyPair {
        let mut mac = HmacSha512::new_from_slice(b"ed25519 seed").unwrap();
        mac.update(seed);
        let mut result = mac.finalize().into_bytes();

        let mut secret = [0u8; 32];
        let mut chain_code = [0u8; 32];
        secret.copy_from_slice(&result[..32]);
        chain_code.copy_from_slice(&result[32..]);
        result.zeroize();

        for &index in path {
            let hardened_index = 0x80000000u32 | index;
            let mut mac = HmacSha512::new_from_slice(&chain_code).unwrap();
            mac.update(&[0x00]);
            mac.update(&secret);
            mac.update(&hardened_index.to_be_bytes());

            secret.zeroize();
            chain_code.zeroize();

            let mut child_result = mac.finalize().into_bytes();
            secret.copy_from_slice(&child_result[..32]);
            chain_code.copy_from_slice(&child_result[32..]);
            child_result.zeroize();
        }

        Ed25519KeyPair { secret, chain_code }
    }

    #[test]
    fn slip0010_test_vector_1_master_key() {
        // Seed: 000102030405060708090a0b0c0d0e0f
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();

        // Chain m
        // private: 2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7
        // chain code: 90046a93de5380a72b5e45010748567d5ea02bbf6522f979e05c0d8d8ca9fffb
        let kp = derive_from_raw_seed(&seed, &[]);

        assert_eq!(
            hex::encode(kp.secret),
            "2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7"
        );
        assert_eq!(
            hex::encode(kp.chain_code),
            "90046a93de5380a72b5e45010748567d5ea02bbf6522f979e05c0d8d8ca9fffb"
        );
    }

    #[test]
    fn slip0010_test_vector_1_child_0h() {
        // Chain m/0'
        // private: 68e0fe46dfb67e368c75379acec591dad19df3cde26e63b93a8e70ffe136eb17
        // chain code: 8b59aa11380b624e81507a27fedda59fea6d0b779a778918a2fd3590e16e9c69
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let kp = derive_from_raw_seed(&seed, &[0]);

        assert_eq!(
            hex::encode(kp.secret),
            "68e0fe46dfb67e368c75379acec591dad19df3cde26e63b93a8e704f1dade7a3"
        );
        assert_eq!(
            hex::encode(kp.chain_code),
            "8b59aa11380b624e81507a27fedda59fea6d0b779a778918a2fd3590e16e9c69"
        );
    }

    #[test]
    fn slip0010_test_vector_1_child_0h_1h() {
        // Chain m/0'/1'
        // private: b1d0bad404bf35da785a64ca1ac54b2617211d2777696fbffaf208f746ae84f2
        // chain code: a320425f77d1b5c2505a6b1b27382b37368ee640e3557c315416801243552f14
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let kp = derive_from_raw_seed(&seed, &[0, 1]);

        assert_eq!(
            hex::encode(kp.secret),
            "b1d0bad404bf35da785a64ca1ac54b2617211d2777696fbffaf208f746ae84f2"
        );
        assert_eq!(
            hex::encode(kp.chain_code),
            "a320425f77d1b5c2505a6b1b27382b37368ee640e3557c315416801243552f14"
        );
    }

    #[test]
    fn slip0010_test_vector_1_deep_path() {
        // Chain m/0'/1'/2'
        // private: 92a5b23c0b8a99e37d07df3fb9966917f5d06e02ddbd909c7e184371463e9fc9
        // chain code: 2e69929e00b5ab250f49c3fb1c12f252de4fed2c1db88387094a0f8c4c9ccd6c
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let kp = derive_from_raw_seed(&seed, &[0, 1, 2]);

        assert_eq!(
            hex::encode(kp.secret),
            "92a5b23c0b8a99e37d07df3fb9966917f5d06e02ddbd909c7e184371463e9fc9"
        );
        assert_eq!(
            hex::encode(kp.chain_code),
            "2e69929e00b5ab250f49c3fb1c12f252de4fed2c1db88387094a0f8c4c9ccd6c"
        );
    }

    #[test]
    fn keypair_debug_is_redacted() {
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let kp = derive_from_raw_seed(&seed, &[]);
        let debug = format!("{:?}", kp);
        assert_eq!(debug, "Ed25519KeyPair(***)");
    }

    #[test]
    fn derive_ed25519_with_64byte_seed() {
        // Use the standard "abandon..." mnemonic seed (64 bytes)
        let seed_hex = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";
        let bytes = hex::decode(seed_hex).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        let seed = SeedBytes(seed);

        let path = solana_path(0);
        let kp = derive_ed25519_keypair(&seed, &path).unwrap();

        // Should produce a valid 32-byte public key
        let pubkey = kp.public_key_bytes();
        assert_eq!(pubkey.len(), 32);

        // Different index should produce different key
        let path1 = solana_path(1);
        let kp1 = derive_ed25519_keypair(&seed, &path1).unwrap();
        assert_ne!(kp.public_key_bytes(), kp1.public_key_bytes());
    }

    #[test]
    fn solana_path_format() {
        assert_eq!(solana_path(0), vec![44, 501, 0, 0]);
        assert_eq!(solana_path(5), vec![44, 501, 5, 0]);
    }
}
