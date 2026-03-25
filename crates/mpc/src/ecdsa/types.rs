//! Types for CGGMP21 threshold ECDSA key shares and signatures.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Encrypted ECDSA key share for storage.
///
/// The raw key share is never stored in plaintext.
/// Encrypted with AES-256-GCM, key derived from party-specific password.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct EncryptedEcdsaShare {
    /// Party index (1-based).
    pub party_index: u16,
    /// Key group identifier.
    pub key_id: String,
    /// AES-256-GCM encrypted share data.
    pub ciphertext: Vec<u8>,
    /// Salt for PBKDF2 key derivation.
    pub salt: Vec<u8>,
    /// AES-GCM nonce.
    pub nonce: Vec<u8>,
}

/// Group public key derived from DKG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcdsaGroupKey {
    /// Key group identifier.
    pub key_id: String,
    /// Compressed secp256k1 public key (33 bytes).
    pub public_key: Vec<u8>,
    /// Threshold (minimum signers).
    pub threshold: u16,
    /// Total parties.
    pub parties: u16,
}

impl std::fmt::Display for EcdsaGroupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EcdsaGroupKey(id={}, t={}/{}, pubkey={}...)",
            self.key_id,
            self.threshold,
            self.parties,
            hex::encode(&self.public_key[..4])
        )
    }
}
