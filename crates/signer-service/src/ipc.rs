//! Unix domain socket IPC server for the signer service.

use std::path::PathBuf;
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::protocol::*;

/// IPC server that listens on a Unix domain socket.
pub struct IpcServer {
    socket_path: PathBuf,
    seed: [u8; 64],
    ipc_secret: Vec<u8>,
}

impl IpcServer {
    pub fn new(socket_path: PathBuf, seed: [u8; 64], ipc_secret: Vec<u8>) -> Self {
        Self { socket_path, seed, ipc_secret }
    }

    pub async fn run(&self) -> Result<()> {
        // Remove stale socket file
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!(path = %self.socket_path.display(), "IPC server listening");

        // Set permissions: owner only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path,
                std::fs::Permissions::from_mode(0o600))?;
        }

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let seed = self.seed;
                    let secret = self.ipc_secret.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, &seed, &secret).await {
                            tracing::error!(error = %e, "IPC connection error");
                        }
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "IPC accept error");
                }
            }
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.seed.zeroize();
        self.ipc_secret.zeroize();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    seed: &[u8; 64],
    ipc_secret: &[u8],
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 { break; } // connection closed

        let line = line.trim();
        if line.is_empty() { continue; }

        // Parse request
        let request: SignRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = SignResponse {
                    request_id: Uuid::nil(),
                    result: SignResult::Error {
                        code: 1001,
                        message: format!("Invalid request: {e}"),
                    },
                    hmac: String::new(),
                };
                let resp_json = serde_json::to_string(&err_resp)? + "\n";
                writer.write_all(resp_json.as_bytes()).await?;
                continue;
            }
        };

        // Verify HMAC
        let op_json = serde_json::to_string(&request.operation)?;
        if !verify_hmac(ipc_secret, &request.request_id, &op_json, &request.hmac) {
            tracing::warn!(request_id = %request.request_id, "IPC HMAC verification failed");
            let err_resp = SignResponse {
                request_id: request.request_id,
                result: SignResult::Error {
                    code: 1004,
                    message: "HMAC verification failed".into(),
                },
                hmac: String::new(),
            };
            let resp_json = serde_json::to_string(&err_resp)? + "\n";
            writer.write_all(resp_json.as_bytes()).await?;
            continue;
        }

        // Process signing request
        let result = process_request(seed, &request);

        // Build response with HMAC
        let result_json = serde_json::to_string(&result)?;
        let resp_hmac = compute_hmac(ipc_secret, &request.request_id, &result_json);

        let response = SignResponse {
            request_id: request.request_id,
            result,
            hmac: resp_hmac,
        };

        let resp_json = serde_json::to_string(&response)? + "\n";
        writer.write_all(resp_json.as_bytes()).await?;

        tracing::info!(
            request_id = %request.request_id,
            chain = %request.chain,
            "Signing request processed"
        );
    }

    Ok(())
}

fn process_request(seed: &[u8; 64], request: &SignRequest) -> SignResult {
    match &request.operation {
        SignOperation::GetPublicKey { derivation_index } => {
            match request.chain {
                custody_common::Chain::Ethereum => {
                    match custody_signing::ethereum::eth_address(seed, *derivation_index) {
                        Ok(addr) => SignResult::Success {
                            signed_data: format!("{addr:#x}"),
                            public_key: Some(format!("{addr:#x}")),
                        },
                        Err(e) => SignResult::Error {
                            code: 5002,
                            message: e.to_string(),
                        },
                    }
                }
                custody_common::Chain::Solana => {
                    match custody_signing::solana::sol_address(seed, *derivation_index) {
                        Ok(pubkey) => SignResult::Success {
                            signed_data: hex::encode(pubkey),
                            public_key: Some(hex::encode(pubkey)),
                        },
                        Err(e) => SignResult::Error {
                            code: 5002,
                            message: e.to_string(),
                        },
                    }
                }
                _ => SignResult::Success {
                    signed_data: String::new(),
                    public_key: None,
                },
            }
        }
        SignOperation::RawSign { derivation_index, message_hash } => {
            let hash_bytes = match hex::decode(message_hash) {
                Ok(b) if b.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&b);
                    arr
                }
                _ => return SignResult::Error {
                    code: 5004,
                    message: "Invalid message hash (need 32 bytes hex)".into(),
                },
            };

            match request.chain {
                custody_common::Chain::Solana => {
                    match custody_signing::raw::sign_raw_ed25519(seed, *derivation_index, &hash_bytes) {
                        Ok((sig, pk)) => SignResult::Success {
                            signed_data: hex::encode(&sig),
                            public_key: Some(hex::encode(&pk)),
                        },
                        Err(e) => SignResult::Error { code: 5001, message: e.to_string() },
                    }
                }
                chain => {
                    match custody_signing::raw::sign_raw_secp256k1(seed, chain, *derivation_index, &hash_bytes) {
                        Ok((sig, pk)) => SignResult::Success {
                            signed_data: hex::encode(&sig),
                            public_key: Some(hex::encode(&pk)),
                        },
                        Err(e) => SignResult::Error { code: 5001, message: e.to_string() },
                    }
                }
            }
        }
        // For full transaction signing, delegate to the appropriate signing module
        _ => SignResult::Error {
            code: 5003,
            message: "Operation not yet implemented in signer service".into(),
        },
    }
}
