//! Bitcoin P2WPKH (Pay-to-Witness-Public-Key-Hash) address encoding.
//!
//! Produces native SegWit Bech32 addresses (BIP-141, BIP-173, BIP-84).
//!
//! Algorithm:
//!   compressed_pubkey (33 bytes) → SHA256 → RIPEMD160 → 20-byte witness program
//!   → Bech32 encode with witness version 0
//!
//! References:
//! - BIP-141: <https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki>
//! - BIP-173: <https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki>
//! - BIP-84:  <https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki>

use anyhow::{ensure, Context, Result};
use bech32::Hrp;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

/// Bitcoin network selection for address encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
}

impl BitcoinNetwork {
    fn hrp(&self) -> Hrp {
        match self {
            BitcoinNetwork::Mainnet => Hrp::parse("bc").expect("valid HRP"),
            BitcoinNetwork::Testnet => Hrp::parse("tb").expect("valid HRP"),
        }
    }
}

/// Encode a compressed secp256k1 public key as a Bitcoin P2WPKH Bech32 address.
///
/// The input must be exactly 33 bytes (compressed SEC1 format).
pub fn encode_p2wpkh(compressed_pubkey: &[u8], network: BitcoinNetwork) -> Result<String> {
    ensure!(
        compressed_pubkey.len() == 33,
        "P2WPKH requires a 33-byte compressed public key, got {}",
        compressed_pubkey.len()
    );
    ensure!(
        compressed_pubkey[0] == 0x02 || compressed_pubkey[0] == 0x03,
        "Invalid compressed public key prefix: 0x{:02x}",
        compressed_pubkey[0]
    );

    // SHA256(compressed_pubkey)
    let sha256_hash = Sha256::digest(compressed_pubkey);

    // RIPEMD160(SHA256(compressed_pubkey))
    let witness_program = Ripemd160::digest(sha256_hash);

    // SegWit v0 P2WPKH: Bech32 encoding (not Bech32m, which is for v1+)
    // bech32 0.11: segwit::encode_v0 encodes witness version 0 programs
    let witness_bytes: &[u8] = witness_program.as_ref();
    let addr = bech32::segwit::encode_v0(network.hrp(), witness_bytes)
        .context("Bech32 segwit encoding failed")?;

    Ok(addr)
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
    fn p2wpkh_abandon_mnemonic_index_0() {
        // "abandon..." mnemonic, m/84'/0'/0'/0/0
        // Expected: bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu
        let seed = test_seed();
        let path = derivation_path(Chain::Bitcoin, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_p2wpkh(&pubkey.compressed, BitcoinNetwork::Mainnet).unwrap();

        assert_eq!(address, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
    }

    #[test]
    fn p2wpkh_abandon_mnemonic_index_1() {
        let seed = test_seed();
        let path = derivation_path(Chain::Bitcoin, 1).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_p2wpkh(&pubkey.compressed, BitcoinNetwork::Mainnet).unwrap();

        // Address should start with bc1q (SegWit v0)
        assert!(address.starts_with("bc1q"), "Expected bc1q prefix, got {}", address);
        // Should be different from index 0
        assert_ne!(address, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
    }

    #[test]
    fn p2wpkh_testnet_prefix() {
        let seed = test_seed();
        let path = derivation_path(Chain::Bitcoin, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();
        let address = encode_p2wpkh(&pubkey.compressed, BitcoinNetwork::Testnet).unwrap();

        assert!(address.starts_with("tb1q"), "Expected tb1q prefix, got {}", address);
    }

    #[test]
    fn p2wpkh_rejects_uncompressed_key() {
        let uncompressed = [0x04u8; 65];
        assert!(encode_p2wpkh(&uncompressed, BitcoinNetwork::Mainnet).is_err());
    }

    #[test]
    fn p2wpkh_rejects_wrong_size() {
        let short = [0x02u8; 20];
        assert!(encode_p2wpkh(&short, BitcoinNetwork::Mainnet).is_err());
    }
}
