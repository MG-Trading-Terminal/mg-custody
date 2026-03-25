//! Software signing backend for development and testing.
//!
//! Implements `SigningBackend` using in-memory key material.
//! **NOT for production use** — production should use HSM or MPC backends.
//!
//! The software backend holds the seed in memory (zeroized on drop)
//! and derives signing keys on demand via BIP-32/SLIP-0010.

use anyhow::Result;
use custody_common::{Chain, CustodyError, CustodyResult};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::SigningBackend;

/// Software-based signing backend holding seed material in memory.
///
/// Suitable for development and testing only.
/// All key material is zeroized on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SoftwareSigningBackend {
    seed: [u8; 64],
}

impl std::fmt::Debug for SoftwareSigningBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SoftwareSigningBackend(***)")
    }
}

impl SoftwareSigningBackend {
    /// Create a new software signing backend from a seed.
    ///
    /// The seed is copied and the original should be zeroized by the caller.
    pub fn new(seed: [u8; 64]) -> Self {
        tracing::warn!("Using software signing backend — NOT for production");
        Self { seed }
    }

    /// Get a reference to the seed (for address derivation coordination).
    ///
    /// # Safety
    /// The caller must not log or persist the returned bytes.
    pub fn seed(&self) -> &[u8; 64] {
        &self.seed
    }
}

impl SigningBackend for SoftwareSigningBackend {
    fn sign(&self, chain: Chain, unsigned_tx: &[u8]) -> CustodyResult<Vec<u8>> {
        // TODO: Implement actual transaction signing per chain:
        //   - Bitcoin: sign SIGHASH with secp256k1 key at derivation path
        //   - Ethereum: sign RLP-encoded tx with secp256k1 key
        //   - Tron: sign protobuf tx with secp256k1 key
        //   - Solana: sign message with Ed25519 key
        //
        // For now, return an error indicating this is not yet implemented.
        // Phase 2 (Ethereum MVP) will implement ETH signing first.
        let _ = (chain, unsigned_tx);
        Err(CustodyError::SigningError(format!(
            "Software signing for {chain:?} not yet implemented (Phase 2)"
        )))
    }
}

/// HSM signing backend stub.
///
/// Placeholder for PKCS#11-based HSM integration (SoftHSM2 for dev, Thales/Utimaco for prod).
/// Will be implemented when HSM infrastructure is available.
pub struct HsmSigningBackend {
    _private: (), // prevent construction outside this module
}

impl std::fmt::Debug for HsmSigningBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HsmSigningBackend(not_configured)")
    }
}

impl HsmSigningBackend {
    /// Create an HSM signing backend.
    ///
    /// # Errors
    /// Currently always returns an error because HSM is not yet implemented.
    pub fn new(_pkcs11_library_path: &str, _slot_id: u64) -> Result<Self> {
        anyhow::bail!("HSM signing backend not yet implemented — use SoftwareSigningBackend for development")
    }
}

impl SigningBackend for HsmSigningBackend {
    fn sign(&self, _chain: Chain, _unsigned_tx: &[u8]) -> CustodyResult<Vec<u8>> {
        Err(CustodyError::SigningError(
            "HSM signing backend not configured".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let mut seed = [0u8; 64];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        seed
    }

    #[test]
    fn software_backend_creation() {
        let backend = SoftwareSigningBackend::new(test_seed());
        assert_eq!(backend.seed(), &test_seed());
    }

    #[test]
    fn software_backend_debug_redacted() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let debug = format!("{:?}", backend);
        assert_eq!(debug, "SoftwareSigningBackend(***)");
    }

    #[test]
    fn software_backend_sign_not_yet_implemented() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let result = backend.sign(Chain::Ethereum, &[0xDE, 0xAD]);
        assert!(result.is_err());
    }

    #[test]
    fn hsm_backend_not_available() {
        let result = HsmSigningBackend::new("/usr/lib/softhsm/libsofthsm2.so", 0);
        assert!(result.is_err());
    }

    #[test]
    fn software_backend_zeroize_on_drop() {
        // We can't easily test zeroize-on-drop in safe Rust,
        // but we verify the type derives Zeroize + ZeroizeOnDrop
        // by ensuring it compiles with those derives.
        let mut backend = SoftwareSigningBackend::new(test_seed());
        backend.zeroize(); // explicit zeroize should work
        assert_eq!(backend.seed(), &[0u8; 64]); // seed should be zeroed
    }
}
