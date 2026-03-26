//! Multi-Party Computation (MPC) threshold signing.
//!
//! Implements 2-of-3 threshold signing for production custody:
//! - **CGGMP21 threshold ECDSA** for Bitcoin, Ethereum, Tron (secp256k1)
//! - **FROST threshold EdDSA** for Solana (Ed25519)
//!
//! Architecture:
//!   Share 1: Application server (encrypted in memory)
//!   Share 2: HSM / Cloud KMS (hardware-protected)
//!   Share 3: Offline backup (cold storage)
//!
//! Any 2 of 3 shares can produce a valid signature.
//! A single compromised share reveals nothing about the private key.
//!
//! Libraries:
//! - `cggmp21` v0.6 (audited by Kudelski Security) for ECDSA
//! - `frost-ed25519` v2.2 (audited by NCC Group, IETF RFC 9591) for EdDSA

pub mod backup;
pub mod ceremony;
pub mod dkg;
pub mod ecdsa;
pub mod frost;
pub mod hsm;
pub mod key_rotation;
pub mod presigning;
pub mod coordinator;
pub mod key_store;
pub mod share_store;
pub mod transport;

use anyhow::Result;
use custody_common::Chain;

/// Result of a distributed key generation ceremony.
#[derive(Debug)]
pub struct KeygenResult {
    /// Unique identifier for this key group.
    pub key_id: String,
    /// The group public key (can be used to derive addresses).
    pub public_key: Vec<u8>,
    /// Number of shares generated.
    pub num_shares: u16,
    /// Minimum shares needed to sign (threshold).
    pub threshold: u16,
}

/// A threshold signature produced by MPC signing.
#[derive(Debug, Clone)]
pub struct ThresholdSignature {
    /// The signature bytes (format depends on scheme: ECDSA or EdDSA).
    pub signature: Vec<u8>,
    /// Which shares participated in signing.
    pub signers: Vec<u16>,
}

/// Trait for MPC signing operations.
///
/// Abstracts over the specific MPC protocol (CGGMP21 for ECDSA, FROST for EdDSA)
/// so the withdrawal pipeline can sign without knowing the protocol details.
pub trait MpcSigner: Send + Sync {
    /// Which chain this signer handles.
    fn chain(&self) -> Chain;

    /// Run distributed key generation with the given threshold and party count.
    fn keygen(
        &self,
        threshold: u16,
        parties: u16,
    ) -> impl std::future::Future<Output = Result<KeygenResult>> + Send;

    /// Sign a message hash using the specified key shares.
    ///
    /// `key_id` identifies the key group from keygen.
    /// `message` is the hash to sign (32 bytes for both ECDSA and EdDSA).
    fn sign(
        &self,
        key_id: &str,
        message: &[u8],
    ) -> impl std::future::Future<Output = Result<ThresholdSignature>> + Send;
}
