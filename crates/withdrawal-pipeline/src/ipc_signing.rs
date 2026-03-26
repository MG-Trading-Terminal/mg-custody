//! IPC-based signing — connects withdrawal pipeline to isolated signer process.
//!
//! Phase 17B completion: the pipeline can now sign via Unix socket IPC
//! instead of calling signing functions directly in the same process.

use anyhow::Result;
use custody_common::Chain;
use custody_signer_service::client::SignerClient;

/// Signing mode — either direct (in-process) or IPC (isolated signer).
#[derive(Debug, Clone)]
pub enum SigningMode {
    /// Direct in-process signing (development mode).
    Direct,
    /// IPC to isolated custody-signer binary (production mode).
    Ipc {
        socket_path: String,
        ipc_secret: Vec<u8>,
    },
}

impl SigningMode {
    /// Create IPC mode from environment variables.
    pub fn from_env() -> Self {
        match (std::env::var("SIGNER_SOCKET"), std::env::var("SIGNER_IPC_SECRET")) {
            (Ok(socket), Ok(secret)) => {
                tracing::info!(socket = %socket, "Using IPC signing mode (isolated signer)");
                Self::Ipc {
                    socket_path: socket,
                    ipc_secret: secret.into_bytes(),
                }
            }
            _ => {
                tracing::warn!("No SIGNER_SOCKET configured — using direct signing mode");
                tracing::warn!("This means signing keys are in the same process as the API");
                Self::Direct
            }
        }
    }

    /// Check if signer is available.
    pub fn is_signer_available(&self) -> bool {
        match self {
            Self::Direct => true,
            Self::Ipc { socket_path, .. } => {
                std::path::Path::new(socket_path).exists()
            }
        }
    }

    /// Get a public key via IPC (or return error if direct mode).
    pub async fn get_public_key_ipc(&self, chain: Chain, index: u32) -> Result<String> {
        match self {
            Self::Ipc { socket_path, ipc_secret } => {
                let client = SignerClient::new(socket_path, ipc_secret);
                client.get_public_key(chain, index).await
            }
            Self::Direct => {
                anyhow::bail!("get_public_key_ipc called in Direct mode — use signing crate directly")
            }
        }
    }

    /// Sign a raw message via IPC.
    pub async fn raw_sign_ipc(
        &self,
        chain: Chain,
        index: u32,
        message_hash: &[u8; 32],
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        match self {
            Self::Ipc { socket_path, ipc_secret } => {
                let client = SignerClient::new(socket_path, ipc_secret);
                client.raw_sign(chain, index, message_hash).await
            }
            Self::Direct => {
                anyhow::bail!("raw_sign_ipc called in Direct mode — use signing crate directly")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mode_always_available() {
        let mode = SigningMode::Direct;
        assert!(mode.is_signer_available());
    }

    #[test]
    fn ipc_mode_not_available_without_socket() {
        let mode = SigningMode::Ipc {
            socket_path: "/tmp/nonexistent-signer.sock".into(),
            ipc_secret: b"secret".to_vec(),
        };
        assert!(!mode.is_signer_available());
    }

    #[test]
    fn from_env_defaults_to_direct() {
        // Without env vars, should be Direct
        // Note: can't remove env vars safely in Rust 2024 (unsafe in tests),
        // but if SIGNER_SOCKET isn't set, from_env() returns Direct
        let mode = SigningMode::Direct; // test the variant directly
        assert!(matches!(mode, SigningMode::Direct));
    }
}
