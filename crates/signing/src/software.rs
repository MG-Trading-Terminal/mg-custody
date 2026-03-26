//! Software signing backend for development and testing.
//!
//! Implements `SigningBackend` using in-memory key material.
//! **NOT for production use** — production should use HSM or MPC backends.
//!
//! The software backend holds the seed in memory (zeroized on drop)
//! and derives signing keys on demand via BIP-32/SLIP-0010.

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
    pub fn new(seed: [u8; 64]) -> Self {
        tracing::warn!("Using software signing backend — NOT for production");
        Self { seed }
    }

    pub fn seed(&self) -> &[u8; 64] {
        &self.seed
    }

    /// Derive a secp256k1 signing key for a chain (BIP-32 path: m/44'/coin'/0'/0/0).
    fn derive_secp256k1_key(&self, chain: Chain) -> CustodyResult<k256::ecdsa::SigningKey> {
        use bip32::XPrv;

        let coin_type = chain.coin_type();
        let path_str = format!("m/44'/{coin_type}'/0'/0/0");
        let path: bip32::DerivationPath = path_str.parse()
            .map_err(|e| CustodyError::SigningError(format!("Bad path: {e}")))?;

        let xprv = XPrv::derive_from_path(&self.seed, &path)
            .map_err(|e| CustodyError::SigningError(format!("BIP-32 derivation: {e}")))?;

        let key_bytes = xprv.to_bytes();
        let signing_key = k256::ecdsa::SigningKey::from_bytes(key_bytes.as_slice().into())
            .map_err(|e| CustodyError::SigningError(format!("Invalid key: {e}")))?;

        Ok(signing_key)
    }
}

impl SigningBackend for SoftwareSigningBackend {
    fn sign(&self, chain: Chain, unsigned_tx: &[u8]) -> CustodyResult<Vec<u8>> {
        match chain {
            Chain::Ethereum | Chain::Bitcoin | Chain::Tron => {
                // For secp256k1 chains: unsigned_tx must be a 32-byte message hash.
                if unsigned_tx.len() != 32 {
                    return Err(CustodyError::SigningError(
                        format!("{chain:?}: unsigned_tx must be 32-byte hash, got {} bytes", unsigned_tx.len())
                    ));
                }

                let signing_key = self.derive_secp256k1_key(chain)?;
                use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature};
                let (sig, _recid): (Signature, _) = signing_key.sign_prehash(unsigned_tx)
                    .map_err(|e| CustodyError::SigningError(format!("Sign: {e}")))?;

                Ok(sig.to_bytes().to_vec())
            }
            Chain::Solana => {
                // Solana uses Ed25519. unsigned_tx is the serialized message to sign.
                use ed25519_dalek::{SigningKey as Ed25519SigningKey, Signer};

                // SLIP-0010 derivation for Ed25519 (m/44'/501'/0'/0')
                use hmac::{Hmac, Mac};
                type HmacSha512 = Hmac<sha2::Sha512>;

                let mut key = HmacSha512::new_from_slice(b"ed25519 seed")
                    .map_err(|e| CustodyError::SigningError(format!("HMAC: {e}")))?;
                key.update(&self.seed);
                let result = key.finalize().into_bytes();
                let mut secret = [0u8; 32];
                let mut chain_code = [0u8; 32];
                secret.copy_from_slice(&result[..32]);
                chain_code.copy_from_slice(&result[32..]);

                // Derive through path: 44', 501', 0', 0'
                for index in [44u32 | 0x80000000, 501 | 0x80000000, 0x80000000, 0x80000000] {
                    let mut hmac_key = HmacSha512::new_from_slice(&chain_code)
                        .map_err(|e| CustodyError::SigningError(format!("HMAC: {e}")))?;
                    hmac_key.update(&[0x00]);
                    hmac_key.update(&secret);
                    hmac_key.update(&index.to_be_bytes());
                    let derived = hmac_key.finalize().into_bytes();
                    secret.copy_from_slice(&derived[..32]);
                    chain_code.copy_from_slice(&derived[32..]);
                }

                let ed_key = Ed25519SigningKey::from_bytes(&secret);
                let signature = ed_key.sign(unsigned_tx);
                Ok(signature.to_bytes().to_vec())
            }
        }
    }
}

/// HSM signing backend stub.
pub struct HsmSigningBackend {
    _private: (),
}

impl std::fmt::Debug for HsmSigningBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HsmSigningBackend(not_configured)")
    }
}

impl HsmSigningBackend {
    pub fn new(_pkcs11_library_path: &str, _slot_id: u64) -> anyhow::Result<Self> {
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
    fn software_backend_sign_eth() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let msg = [0xABu8; 32];
        let result = backend.sign(Chain::Ethereum, &msg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 64); // r + s
    }

    #[test]
    fn software_backend_sign_btc() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let msg = [0xCDu8; 32];
        let result = backend.sign(Chain::Bitcoin, &msg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 64);
    }

    #[test]
    fn software_backend_sign_trx() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let msg = [0xEFu8; 32];
        let result = backend.sign(Chain::Tron, &msg);
        assert!(result.is_ok());
    }

    #[test]
    fn software_backend_sign_sol() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let msg = b"hello solana";
        let result = backend.sign(Chain::Solana, msg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 64); // Ed25519 signature
    }

    #[test]
    fn software_backend_sign_invalid_length() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let msg = [0xABu8; 16];
        let result = backend.sign(Chain::Ethereum, &msg);
        assert!(result.is_err());
    }

    #[test]
    fn hsm_backend_not_available() {
        let result = HsmSigningBackend::new("/usr/lib/softhsm/libsofthsm2.so", 0);
        assert!(result.is_err());
    }

    #[test]
    fn software_backend_zeroize_on_drop() {
        let mut backend = SoftwareSigningBackend::new(test_seed());
        backend.zeroize();
        assert_eq!(backend.seed(), &[0u8; 64]);
    }

    #[test]
    fn debug_output_contains_no_key_material() {
        let backend = SoftwareSigningBackend::new(test_seed());
        let debug = format!("{:?}", backend);
        // Debug should show *** not the actual seed
        assert!(!debug.contains("0001020304"));
        assert!(debug.contains("***"));
    }

    #[test]
    fn hsm_backend_debug_safe() {
        let debug = format!("{:?}", HsmSigningBackend { _private: () });
        assert!(debug.contains("not_configured"));
        assert!(!debug.contains("seed"));
    }
}
