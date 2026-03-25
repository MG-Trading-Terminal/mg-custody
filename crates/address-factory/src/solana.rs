//! Solana address encoding (Ed25519 public key as Base58).
//!
//! Solana addresses are simply the Base58-encoded 32-byte Ed25519 public key
//! (no checksum, no prefix).
//!
//! Key derivation uses SLIP-0010 (see `slip0010.rs`), not BIP-32.
//! Standard path: m/44'/501'/{account}'/0'

use anyhow::{ensure, Result};

/// Encode a 32-byte Ed25519 public key as a Solana address (Base58, no checksum).
pub fn encode_solana_address(pubkey_bytes: &[u8]) -> Result<String> {
    ensure!(
        pubkey_bytes.len() == 32,
        "Solana address requires a 32-byte Ed25519 public key, got {}",
        pubkey_bytes.len()
    );

    Ok(bs58::encode(pubkey_bytes).into_string())
}

/// Decode a Solana Base58 address to the raw 32-byte public key.
pub fn decode_solana_address(address: &str) -> Result<[u8; 32]> {
    let bytes = bs58::decode(address)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Invalid Base58: {e}"))?;

    ensure!(
        bytes.len() == 32,
        "Solana address must decode to 32 bytes, got {}",
        bytes.len()
    );

    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd::SeedBytes;
    use crate::slip0010;

    fn test_seed() -> SeedBytes {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        SeedBytes(seed)
    }

    #[test]
    fn solana_address_from_abandon_mnemonic() {
        let seed = test_seed();
        let path = slip0010::solana_path(0);
        let kp = slip0010::derive_ed25519_keypair(&seed, &path).unwrap();
        let pubkey = kp.public_key_bytes();
        let address = encode_solana_address(&pubkey).unwrap();

        // Solana addresses are 32-44 characters long (Base58 of 32 bytes)
        assert!(address.len() >= 32 && address.len() <= 44, "Unexpected address length: {}", address.len());
        // Should not contain 0, O, I, l (Base58 alphabet excludes these)
        assert!(!address.contains('0'));
        assert!(!address.contains('O'));
        assert!(!address.contains('I'));
        assert!(!address.contains('l'));
    }

    #[test]
    fn solana_address_roundtrip() {
        let seed = test_seed();
        let path = slip0010::solana_path(0);
        let kp = slip0010::derive_ed25519_keypair(&seed, &path).unwrap();
        let pubkey = kp.public_key_bytes();
        let address = encode_solana_address(&pubkey).unwrap();
        let decoded = decode_solana_address(&address).unwrap();

        assert_eq!(pubkey, decoded);
    }

    #[test]
    fn solana_different_indices() {
        let seed = test_seed();
        let path0 = slip0010::solana_path(0);
        let path1 = slip0010::solana_path(1);
        let kp0 = slip0010::derive_ed25519_keypair(&seed, &path0).unwrap();
        let kp1 = slip0010::derive_ed25519_keypair(&seed, &path1).unwrap();
        let addr0 = encode_solana_address(&kp0.public_key_bytes()).unwrap();
        let addr1 = encode_solana_address(&kp1.public_key_bytes()).unwrap();

        assert_ne!(addr0, addr1);
    }

    #[test]
    fn solana_rejects_wrong_size() {
        assert!(encode_solana_address(&[0u8; 20]).is_err());
        assert!(encode_solana_address(&[0u8; 33]).is_err());
    }
}
