//! HD wallet address derivation and generation.
//!
//! Derives addresses deterministically from a master HD key using BIP32/BIP44:
//!   m/44'/<coin_type>'/<account_index>'/0/<address_index>
//!
//! Supports: Bitcoin (P2WPKH), Ethereum (EIP-55), Tron (Base58Check), Solana (Ed25519).
