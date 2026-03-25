//! Tron address encoding (Base58Check with 0x41 prefix).
//!
//! Tron uses the same secp256k1 → Keccak-256 address derivation as Ethereum,
//! but encodes with a 0x41 (mainnet) prefix byte and Base58Check instead of hex.
//!
//! Algorithm:
//!   uncompressed_pubkey → Keccak-256 → last 20 bytes → prepend 0x41
//!   → SHA256(SHA256(21 bytes)) → first 4 bytes as checksum → Base58 encode(25 bytes)
//!
//! Reference: <https://developers.tron.network/docs/account>

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::ethereum;

/// Tron mainnet address prefix byte.
const TRON_PREFIX: u8 = 0x41;

/// Encode an uncompressed secp256k1 public key as a Tron mainnet address.
///
/// Returns a Base58Check-encoded address starting with 'T'.
pub fn encode_tron_address(uncompressed_pubkey: &[u8]) -> Result<String> {
    // Reuse Ethereum's Keccak-256 → 20-byte address derivation
    let eth_address = ethereum::pubkey_to_address_bytes(uncompressed_pubkey)?;

    Ok(address_bytes_to_base58check(&eth_address))
}

/// Convert raw 20-byte address to Tron Base58Check format.
///
/// Prepends 0x41, computes double-SHA256 checksum, Base58 encodes.
pub fn address_bytes_to_base58check(address_bytes: &[u8; 20]) -> String {
    // 0x41 prefix + 20-byte address
    let mut payload = Vec::with_capacity(25);
    payload.push(TRON_PREFIX);
    payload.extend_from_slice(address_bytes);

    // Double SHA256 checksum
    let hash1 = Sha256::digest(&payload);
    let hash2 = Sha256::digest(hash1);
    let checksum = &hash2[..4];

    // Append checksum
    payload.extend_from_slice(checksum);

    // Base58 encode (no additional check — we computed it manually)
    bs58::encode(payload).into_string()
}

/// Decode a Tron Base58Check address to the raw 20-byte address.
///
/// Validates the checksum and 0x41 prefix.
pub fn decode_tron_address(address: &str) -> Result<[u8; 20]> {
    let decoded = bs58::decode(address)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Invalid Base58: {e}"))?;

    anyhow::ensure!(decoded.len() == 25, "Tron address must decode to 25 bytes, got {}", decoded.len());
    anyhow::ensure!(decoded[0] == TRON_PREFIX, "Tron address must start with 0x41 prefix");

    // Verify checksum
    let payload = &decoded[..21];
    let expected_checksum = &decoded[21..25];
    let hash1 = Sha256::digest(payload);
    let hash2 = Sha256::digest(hash1);
    let actual_checksum = &hash2[..4];

    anyhow::ensure!(
        expected_checksum == actual_checksum,
        "Tron address checksum mismatch"
    );

    let mut address = [0u8; 20];
    address.copy_from_slice(&decoded[1..21]);
    Ok(address)
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
    fn tron_address_starts_with_t() {
        let seed = test_seed();
        let path = derivation_path(Chain::Tron, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_tron_address(&pubkey.uncompressed).unwrap();

        assert!(
            address.starts_with('T'),
            "Tron mainnet address should start with 'T', got: {address}"
        );
    }

    #[test]
    fn tron_address_roundtrip() {
        let seed = test_seed();
        let path = derivation_path(Chain::Tron, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_tron_address(&pubkey.uncompressed).unwrap();

        // Decode should recover the same 20-byte address
        let decoded = decode_tron_address(&address).unwrap();
        let original = crate::ethereum::pubkey_to_address_bytes(&pubkey.uncompressed).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn tron_and_ethereum_share_20byte_hash() {
        // Tron and Ethereum derive the same 20-byte hash from the same pubkey
        let seed = test_seed();
        let path = derivation_path(Chain::Tron, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();

        let eth_bytes = crate::ethereum::pubkey_to_address_bytes(&pubkey.uncompressed).unwrap();
        let tron_address = encode_tron_address(&pubkey.uncompressed).unwrap();
        let tron_bytes = decode_tron_address(&tron_address).unwrap();

        assert_eq!(eth_bytes, tron_bytes);
    }

    #[test]
    fn tron_different_indices() {
        let seed = test_seed();
        let path0 = derivation_path(Chain::Tron, 0).unwrap();
        let path1 = derivation_path(Chain::Tron, 1).unwrap();
        let pk0 = derive_secp256k1_pubkey(&seed, &path0).unwrap();
        let pk1 = derive_secp256k1_pubkey(&seed, &path1).unwrap();
        let addr0 = encode_tron_address(&pk0.uncompressed).unwrap();
        let addr1 = encode_tron_address(&pk1.uncompressed).unwrap();

        assert_ne!(addr0, addr1);
    }

    #[test]
    fn tron_invalid_checksum_rejected() {
        let seed = test_seed();
        let path = derivation_path(Chain::Tron, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let mut address = encode_tron_address(&pubkey.uncompressed).unwrap();

        // Corrupt the last character
        let last = address.pop().unwrap();
        let replacement = if last == 'A' { 'B' } else { 'A' };
        address.push(replacement);

        assert!(decode_tron_address(&address).is_err());
    }
}
