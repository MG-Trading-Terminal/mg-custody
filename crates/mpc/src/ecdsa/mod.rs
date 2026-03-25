//! CGGMP21 threshold ECDSA for secp256k1 chains (Bitcoin, Ethereum, Tron).
//!
//! Protocol: CGGMP'24 via the `cggmp21` crate (audited by Kudelski Security).
//! Produces standard ECDSA signatures indistinguishable from single-party signatures.

pub mod keygen;
pub mod sign;
pub mod types;

pub use keygen::trusted_dealer_keygen;
pub use sign::threshold_sign;
pub use types::{EcdsaGroupKey, EncryptedEcdsaShare};
