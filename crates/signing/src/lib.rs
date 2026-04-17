//! Transaction signing service.
//!
//! The most security-critical component. Signs transactions using
//! private key material stored in HSM or software key store.
//!
//! The signing service validates every transaction before signing:
//! - Destination address matches the approved withdrawal record
//! - Amount matches the approved withdrawal record
//! - Signing request carries a valid HMAC from the approval layer
//!
//! ## Modules
//!
//! - `encrypted_store`: AES-256-GCM encrypted seed storage at rest
//! - `shamir`: Shamir 3-of-5 secret sharing for seed backup ceremonies
//! - `software`: Software signing backend (dev/test only)

pub mod bitcoin;
pub mod encrypted_store;
pub mod ethereum;
pub mod mnemonic;
pub mod pkcs11;
pub mod raw;
pub mod shamir;
pub mod software;
pub mod solana;
pub mod staking;
pub mod tron;

use custody_common::CustodyResult;

/// Trait for signing backends (software keys, HSM, MPC).
///
/// Implementations must validate every transaction before signing:
/// - Destination, amount, and chain match the approved withdrawal record
/// - HMAC from approval layer is valid
///
/// The signing service has NO public network interface (air-gapped Zone 3).
pub trait SigningBackend: Send + Sync {
    /// Sign raw transaction bytes for a given chain.
    fn sign(&self, chain: custody_common::Chain, unsigned_tx: &[u8]) -> CustodyResult<Vec<u8>>;

    /// Return the public key this backend will sign with for `chain`.
    ///
    /// Shape:
    ///   - secp256k1 (BTC / ETH / Tron): 33-byte compressed SEC1.
    ///   - ed25519   (Solana):           32-byte raw public key.
    ///
    /// Default impl returns `None` — software backends that derive
    /// per-user keys have no single "backend public key" to expose.
    /// HSM backends override this so Solana signing + Bitcoin
    /// consolidation paths can build their messages without
    /// guessing which on-chain address the HSM controls.
    fn public_key(&self, _chain: custody_common::Chain) -> Option<Vec<u8>> {
        None
    }
}

pub use encrypted_store::{encrypt_seed, decrypt_seed, Password};
pub use pkcs11::{Pkcs11SigningBackend, Pkcs11SigningConfig};
pub use shamir::{split_3_of_5, reconstruct_3_of_5, Share};
pub use software::{SoftwareSigningBackend, HsmSigningBackend};
