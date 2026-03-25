//! Types for FROST threshold EdDSA key shares and signatures.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Encrypted FROST key share for storage.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct EncryptedFrostShare {
    /// Party identifier.
    pub party_id: u16,
    /// Key group identifier.
    pub key_id: String,
    /// AES-256-GCM encrypted share data.
    pub ciphertext: Vec<u8>,
    /// Salt for key derivation.
    pub salt: Vec<u8>,
    /// Nonce.
    pub nonce: Vec<u8>,
}

/// FROST group verifying key (Ed25519 public key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostGroupKey {
    /// Key group identifier.
    pub key_id: String,
    /// Ed25519 group public key (32 bytes).
    pub public_key: Vec<u8>,
    /// Threshold.
    pub threshold: u16,
    /// Total parties.
    pub parties: u16,
}
