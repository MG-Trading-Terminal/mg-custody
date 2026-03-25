//! BIP-32/BIP-44 HD key derivation for secp256k1 chains (Bitcoin, Ethereum, Tron).
//!
//! Uses the `bip32` crate for hierarchical deterministic key derivation.
//! Solana (Ed25519) uses SLIP-0010 instead — see `slip0010.rs`.

use anyhow::{Context, Result};
use bip32::{DerivationPath, XPrv};
use k256::ecdsa::VerifyingKey;
use zeroize::{Zeroize, ZeroizeOnDrop};

use custody_common::Chain;

/// Master seed bytes (64 bytes from BIP-39 mnemonic-to-seed).
///
/// Zeroized on drop to prevent key material from lingering in memory.
/// Debug output is redacted to prevent accidental logging of key material.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SeedBytes(pub [u8; 64]);

impl std::fmt::Debug for SeedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeedBytes(***)")
    }
}

/// secp256k1 public key in both compressed and uncompressed forms.
///
/// Compressed (33 bytes) is used for Bitcoin P2WPKH.
/// Uncompressed (65 bytes, with 04 prefix) is used for Ethereum/Tron address derivation.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Secp256k1PubKey {
    pub compressed: [u8; 33],
    pub uncompressed: [u8; 65],
}

/// Build the BIP-44/BIP-84 derivation path for a secp256k1 chain.
///
/// - Bitcoin: m/84'/0'/0'/0/{index}  (BIP-84 for P2WPKH native SegWit)
/// - Ethereum: m/44'/60'/0'/0/{index}
/// - Tron: m/44'/195'/0'/0/{index}
/// - Solana: not handled here (uses SLIP-0010)
pub fn derivation_path(chain: Chain, index: u32) -> Result<DerivationPath> {
    let path_str = match chain {
        Chain::Bitcoin => format!("m/84'/0'/0'/0/{index}"),
        Chain::Ethereum => format!("m/44'/60'/0'/0/{index}"),
        Chain::Tron => format!("m/44'/195'/0'/0/{index}"),
        Chain::Solana => anyhow::bail!("Solana uses SLIP-0010, not BIP-32 secp256k1 derivation"),
    };
    path_str
        .parse::<DerivationPath>()
        .map_err(|e| anyhow::anyhow!("Invalid derivation path '{}': {}", path_str, e))
}

/// Derive a secp256k1 public key at the given BIP-32 derivation path.
///
/// Returns both compressed and uncompressed forms of the public key.
/// The extended private key is zeroized when dropped (handled by `bip32::XPrv`).
pub fn derive_secp256k1_pubkey(seed: &SeedBytes, path: &DerivationPath) -> Result<Secp256k1PubKey> {
    let xprv = XPrv::derive_from_path(&seed.0, path)
        .context("BIP-32 key derivation failed")?;

    // Get the public key from the extended private key
    let xpub = xprv.public_key();
    let verifying_key = VerifyingKey::from(xpub);

    let compressed_point = verifying_key.to_encoded_point(true);
    let uncompressed_point = verifying_key.to_encoded_point(false);

    let mut compressed = [0u8; 33];
    let mut uncompressed = [0u8; 65];

    compressed.copy_from_slice(compressed_point.as_bytes());
    uncompressed.copy_from_slice(uncompressed_point.as_bytes());

    Ok(Secp256k1PubKey {
        compressed,
        uncompressed,
    })
}

/// Build the derivation path string for storage in the database.
pub fn derivation_path_string(chain: Chain, index: u32) -> Result<String> {
    match chain {
        Chain::Bitcoin => Ok(format!("m/84'/0'/0'/0/{index}")),
        Chain::Ethereum => Ok(format!("m/44'/60'/0'/0/{index}")),
        Chain::Tron => Ok(format!("m/44'/195'/0'/0/{index}")),
        Chain::Solana => Ok(format!("m/44'/501'/{index}'/0'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard BIP-39 "abandon..." mnemonic seed (hex).
    /// Mnemonic: abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about
    /// Passphrase: (empty)
    const TEST_SEED_HEX: &str = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";

    fn test_seed() -> SeedBytes {
        let bytes = hex::decode(TEST_SEED_HEX).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        SeedBytes(seed)
    }

    #[test]
    fn seed_debug_is_redacted() {
        let seed = test_seed();
        let debug = format!("{:?}", seed);
        assert_eq!(debug, "SeedBytes(***)");
        assert!(!debug.contains("5eb00b"));
    }

    #[test]
    fn bitcoin_derivation_path() {
        let path = derivation_path(Chain::Bitcoin, 0).unwrap();
        assert_eq!(path.to_string(), "m/84'/0'/0'/0/0");

        let path = derivation_path(Chain::Bitcoin, 5).unwrap();
        assert_eq!(path.to_string(), "m/84'/0'/0'/0/5");
    }

    #[test]
    fn ethereum_derivation_path() {
        let path = derivation_path(Chain::Ethereum, 0).unwrap();
        assert_eq!(path.to_string(), "m/44'/60'/0'/0/0");
    }

    #[test]
    fn tron_derivation_path() {
        let path = derivation_path(Chain::Tron, 0).unwrap();
        assert_eq!(path.to_string(), "m/44'/195'/0'/0/0");
    }

    #[test]
    fn solana_derivation_path_rejected() {
        assert!(derivation_path(Chain::Solana, 0).is_err());
    }

    #[test]
    fn derive_secp256k1_pubkey_bitcoin_index_0() {
        let seed = test_seed();
        let path = derivation_path(Chain::Bitcoin, 0).unwrap();
        let pubkey = derive_secp256k1_pubkey(&seed, &path).unwrap();

        // Compressed key should be 33 bytes starting with 02 or 03
        assert!(pubkey.compressed[0] == 0x02 || pubkey.compressed[0] == 0x03);
        // Uncompressed key should be 65 bytes starting with 04
        assert_eq!(pubkey.uncompressed[0], 0x04);
    }

    #[test]
    fn derive_different_indices_produce_different_keys() {
        let seed = test_seed();
        let path0 = derivation_path(Chain::Ethereum, 0).unwrap();
        let path1 = derivation_path(Chain::Ethereum, 1).unwrap();
        let pk0 = derive_secp256k1_pubkey(&seed, &path0).unwrap();
        let pk1 = derive_secp256k1_pubkey(&seed, &path1).unwrap();

        assert_ne!(pk0.compressed, pk1.compressed);
    }

    #[test]
    fn derive_different_chains_produce_different_keys() {
        let seed = test_seed();
        let btc_path = derivation_path(Chain::Bitcoin, 0).unwrap();
        let eth_path = derivation_path(Chain::Ethereum, 0).unwrap();
        let btc_pk = derive_secp256k1_pubkey(&seed, &btc_path).unwrap();
        let eth_pk = derive_secp256k1_pubkey(&seed, &eth_path).unwrap();

        assert_ne!(btc_pk.compressed, eth_pk.compressed);
    }

    #[test]
    fn derivation_path_string_all_chains() {
        assert_eq!(derivation_path_string(Chain::Bitcoin, 0).unwrap(), "m/84'/0'/0'/0/0");
        assert_eq!(derivation_path_string(Chain::Ethereum, 3).unwrap(), "m/44'/60'/0'/0/3");
        assert_eq!(derivation_path_string(Chain::Tron, 7).unwrap(), "m/44'/195'/0'/0/7");
        assert_eq!(derivation_path_string(Chain::Solana, 2).unwrap(), "m/44'/501'/2'/0'");
    }
}
