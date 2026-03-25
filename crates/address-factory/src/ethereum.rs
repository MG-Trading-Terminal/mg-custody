//! Ethereum address encoding with EIP-55 mixed-case checksum.
//!
//! Algorithm:
//!   uncompressed_pubkey (64 bytes, no 04 prefix) → Keccak-256 → last 20 bytes → hex
//!   EIP-55: uppercase hex character if corresponding nibble of keccak256(hex_lowercase) >= 8
//!
//! Reference: <https://eips.ethereum.org/EIPS/eip-55>

use anyhow::{ensure, Result};
use sha3::{Digest, Keccak256};

/// Derive the 20-byte Ethereum address from an uncompressed secp256k1 public key.
///
/// The input must be exactly 65 bytes (04 || x || y) or 64 bytes (x || y).
/// Returns the raw 20-byte address.
pub fn pubkey_to_address_bytes(uncompressed_pubkey: &[u8]) -> Result<[u8; 20]> {
    // Accept both 65-byte (with 04 prefix) and 64-byte (without prefix) forms
    let pubkey_xy = match uncompressed_pubkey.len() {
        65 => {
            ensure!(
                uncompressed_pubkey[0] == 0x04,
                "65-byte public key must start with 0x04"
            );
            &uncompressed_pubkey[1..]
        }
        64 => uncompressed_pubkey,
        n => anyhow::bail!("Expected 64 or 65 byte uncompressed public key, got {n}"),
    };

    // Keccak-256 of the 64-byte (x, y) coordinates
    let hash = Keccak256::digest(pubkey_xy);

    // Last 20 bytes
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    Ok(address)
}

/// Encode a 20-byte Ethereum address with EIP-55 mixed-case checksum.
///
/// Returns a string like "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed".
pub fn eip55_checksum(address_bytes: &[u8; 20]) -> String {
    let hex_lower = hex::encode(address_bytes);

    // Keccak-256 of the lowercase hex string (without 0x prefix)
    let hash = Keccak256::digest(hex_lower.as_bytes());

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");

    for (i, c) in hex_lower.chars().enumerate() {
        if c.is_ascii_alphabetic() {
            // Each hex character corresponds to 4 bits (a nibble) in the hash
            let hash_byte = hash[i / 2];
            let nibble = if i % 2 == 0 {
                hash_byte >> 4
            } else {
                hash_byte & 0x0F
            };
            if nibble >= 8 {
                checksummed.push(c.to_ascii_uppercase());
            } else {
                checksummed.push(c);
            }
        } else {
            checksummed.push(c);
        }
    }

    checksummed
}

/// Encode an uncompressed secp256k1 public key as an EIP-55 checksummed Ethereum address.
pub fn encode_ethereum_address(uncompressed_pubkey: &[u8]) -> Result<String> {
    let address_bytes = pubkey_to_address_bytes(uncompressed_pubkey)?;
    Ok(eip55_checksum(&address_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd::{derive_secp256k1_pubkey, derivation_path, SeedBytes};
    use custody_common::Chain;

    fn test_seed() -> SeedBytes {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        SeedBytes(seed)
    }

    #[test]
    fn eip55_test_vectors() {
        // EIP-55 test cases from the spec
        let cases = [
            "5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "fB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "dbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "D1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ];

        for expected in &cases {
            let bytes = hex::decode(&expected.to_lowercase()).unwrap();
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&bytes);
            let result = eip55_checksum(&addr);
            assert_eq!(result, format!("0x{expected}"), "EIP-55 mismatch for {expected}");
        }
    }

    #[test]
    fn ethereum_abandon_mnemonic_index_0() {
        // "abandon..." mnemonic, m/44'/60'/0'/0/0
        // Expected: 0x9858EfFD232B4033E47d90003D41EC34EcaEda94
        let seed = test_seed();
        let path = derivation_path(Chain::Ethereum, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_ethereum_address(&pubkey.uncompressed).unwrap();

        assert_eq!(address, "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
    }

    #[test]
    fn ethereum_different_indices() {
        let seed = test_seed();
        let path0 = derivation_path(Chain::Ethereum, 0).unwrap();
        let path1 = derivation_path(Chain::Ethereum, 1).unwrap();
        let pk0 = derive_secp256k1_pubkey(&seed, &path0).unwrap();
        let pk1 = derive_secp256k1_pubkey(&seed, &path1).unwrap();
        let addr0 = encode_ethereum_address(&pk0.uncompressed).unwrap();
        let addr1 = encode_ethereum_address(&pk1.uncompressed).unwrap();

        assert_ne!(addr0, addr1);
        assert!(addr0.starts_with("0x"));
        assert!(addr1.starts_with("0x"));
        assert_eq!(addr0.len(), 42); // "0x" + 40 hex chars
    }

    #[test]
    fn ethereum_rejects_compressed_key() {
        let compressed = [0x02u8; 33];
        assert!(encode_ethereum_address(&compressed).is_err());
    }

    #[test]
    fn ethereum_accepts_64_and_65_byte_keys() {
        let seed = test_seed();
        let path = derivation_path(Chain::Ethereum, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();

        // 65-byte form (with 04 prefix)
        let addr_65 = encode_ethereum_address(&pubkey.uncompressed).unwrap();

        // 64-byte form (without prefix)
        let addr_64 = encode_ethereum_address(&pubkey.uncompressed[1..]).unwrap();

        assert_eq!(addr_65, addr_64);
    }
}
