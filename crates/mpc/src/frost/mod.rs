//! FROST threshold EdDSA for Ed25519 chains (Solana).
//!
//! Protocol: FROST per IETF RFC 9591 via the `frost-ed25519` crate.
//! Audited by NCC Group.
//! Produces standard Ed25519 signatures compatible with Solana.

pub mod keygen;
pub mod sign;
pub mod types;

pub use keygen::trusted_dealer_keygen;
pub use sign::{frost_sign, verify_frost_signature};
pub use types::{EncryptedFrostShare, FrostGroupKey};
