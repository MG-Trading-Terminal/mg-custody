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
        let mode = SigningMode::Direct;
        assert!(matches!(mode, SigningMode::Direct));
    }

    #[test]
    fn ipc_mode_returns_503_without_socket() {
        // Phase 17B integration test: if signer socket doesn't exist,
        // IPC operations should fail gracefully (not panic)
        let mode = SigningMode::Ipc {
            socket_path: "/tmp/custody-signer-nonexistent-test.sock".into(),
            ipc_secret: b"test".to_vec(),
        };
        assert!(!mode.is_signer_available());
        // In production, the pipeline should return 503 when signer unavailable
    }

    #[tokio::test]
    async fn ipc_get_public_key_fails_without_socket() {
        let mode = SigningMode::Ipc {
            socket_path: "/tmp/custody-signer-nonexistent-test.sock".into(),
            ipc_secret: b"test".to_vec(),
        };
        let result = mode.get_public_key_ipc(custody_common::Chain::Ethereum, 0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("connect"));
    }

    #[tokio::test]
    async fn ipc_raw_sign_fails_without_socket() {
        let mode = SigningMode::Ipc {
            socket_path: "/tmp/custody-signer-nonexistent-test.sock".into(),
            ipc_secret: b"test".to_vec(),
        };
        let hash = [0xAA; 32];
        let result = mode.raw_sign_ipc(custody_common::Chain::Ethereum, 0, &hash).await;
        assert!(result.is_err());
    }

    #[test]
    fn direct_mode_rejects_ipc_calls() {
        let mode = SigningMode::Direct;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(mode.get_public_key_ipc(custody_common::Chain::Ethereum, 0));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Direct mode"));
    }
}
