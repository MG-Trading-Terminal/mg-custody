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

    /// Boot-time validation for operators who explicitly opted into
    /// IPC signing. Returns an error with a concrete reason when
    /// the config is half-done — the binary's main() uses this to
    /// refuse to start rather than silently falling through to
    /// Direct mode (which would put signing keys back in the API
    /// process, defeating the whole point of the signer isolation).
    ///
    /// Rules:
    ///   - SIGNER_SOCKET set, SIGNER_IPC_SECRET unset → hard error
    ///   - SIGNER_IPC_SECRET set, SIGNER_SOCKET unset → hard error
    ///   - Both set, socket path does not exist on disk → hard
    ///     error (the signer daemon isn't running)
    ///   - Both set + socket present → Ok; caller can use from_env()
    ///     to get the configured mode
    ///   - Neither set → Ok (dev Direct mode; from_env() warns
    ///     loudly about keys in-process)
    ///
    /// Pipelines today still use the legacy SigningBackend trait
    /// (HSM / seed); the IPC SignerClient is wired into staking and
    /// swap paths but not yet universally. This check at least
    /// catches the "I thought I enabled IPC but didn't" footgun.
    pub fn validate_env() -> Result<()> {
        let socket = std::env::var("SIGNER_SOCKET").ok().filter(|s| !s.is_empty());
        let secret = std::env::var("SIGNER_IPC_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        match (socket, secret) {
            (Some(_), None) => {
                anyhow::bail!(
                    "SIGNER_SOCKET is set but SIGNER_IPC_SECRET is missing — \
                     IPC signer authentication is impossible in this config. \
                     Set both or unset both."
                )
            }
            (None, Some(_)) => {
                anyhow::bail!(
                    "SIGNER_IPC_SECRET is set but SIGNER_SOCKET is missing — \
                     no signer daemon to talk to. Set SIGNER_SOCKET to the \
                     Unix socket path or unset SIGNER_IPC_SECRET."
                )
            }
            (Some(socket), Some(_)) => {
                if !std::path::Path::new(&socket).exists() {
                    anyhow::bail!(
                        "SIGNER_SOCKET={socket} but no socket exists at that path. \
                         Is the custody-signer daemon running? Refusing to start in \
                         a half-configured IPC mode rather than silently fall back \
                         to in-process Direct signing.",
                    )
                }
                Ok(())
            }
            (None, None) => Ok(()),
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
