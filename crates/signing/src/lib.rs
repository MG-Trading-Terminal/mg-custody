//! Transaction signing service.
//!
//! The most security-critical component. Signs transactions using
//! private key material stored in HSM or software key store.
//!
//! The signing service validates every transaction before signing:
//! - Destination address matches the approved withdrawal record
//! - Amount matches the approved withdrawal record
//! - Signing request carries a valid HMAC from the approval layer

use custody_common::CustodyResult;

/// Trait for signing backends (software keys, HSM, MPC).
pub trait SigningBackend: Send + Sync {
    /// Sign raw transaction bytes for a given chain.
    fn sign(&self, chain: custody_common::Chain, unsigned_tx: &[u8]) -> CustodyResult<Vec<u8>>;
}
