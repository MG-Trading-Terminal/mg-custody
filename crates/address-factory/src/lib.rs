//! HD wallet address derivation and generation.
//!
//! Derives addresses deterministically from a master HD key:
//! - Bitcoin, Ethereum, Tron: BIP-32/BIP-44 secp256k1 derivation
//! - Solana: SLIP-0010 Ed25519 derivation (hardened-only)
//!
//! Derivation paths:
//! - Bitcoin:  m/84'/0'/0'/0/{index}   (BIP-84, P2WPKH native SegWit)
//! - Ethereum: m/44'/60'/0'/0/{index}  (BIP-44)
//! - Tron:     m/44'/195'/0'/0/{index} (BIP-44)
//! - Solana:   m/44'/501'/{index}'/0'  (SLIP-0010)
//!
//! Security: All key material types implement `Zeroize` + `ZeroizeOnDrop`.
//! Key material NEVER appears in logs or Debug output.

pub mod hd;
pub mod slip0010;
pub mod bitcoin;
pub mod ethereum;
pub mod tron;
pub mod solana;
pub mod allocator;

pub use allocator::{
    AddressAllocator, AddressFactoryConfig, chain_and_index_from_path, derive_address,
    pubkey_hex_for_index,
};
pub use bitcoin::BitcoinNetwork;
pub use hd::SeedBytes;
pub use slip0010::Ed25519KeyPair;
