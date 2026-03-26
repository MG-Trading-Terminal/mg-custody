//! IPC client — used by the main custody binary to talk to custody-signer.
//!
//! Connects to the Unix domain socket and sends HMAC-authenticated signing requests.

use std::path::Path;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use uuid::Uuid;

use crate::protocol::*;

/// Client for communicating with the isolated custody-signer process.
pub struct SignerClient {
    socket_path: String,
    ipc_secret: Vec<u8>,
}

impl SignerClient {
    pub fn new(socket_path: &str, ipc_secret: &[u8]) -> Self {
        Self {
            socket_path: socket_path.into(),
            ipc_secret: ipc_secret.to_vec(),
        }
    }

    /// Check if the signer socket is available.
    pub fn is_available(&self) -> bool {
        Path::new(&self.socket_path).exists()
    }

    /// Send a signing request and get the response.
    pub async fn request(&self, chain: custody_common::Chain, operation: SignOperation) -> Result<SignResult> {
        let stream = UnixStream::connect(&self.socket_path).await
            .context("Failed to connect to custody-signer socket")?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let request_id = Uuid::new_v4();
        let op_json = serde_json::to_string(&operation)?;
        let hmac = compute_hmac(&self.ipc_secret, &request_id, &op_json);

        let request = SignRequest {
            request_id,
            chain,
            operation,
            hmac,
        };

        // Send request as single line of JSON
        let req_json = serde_json::to_string(&request)? + "\n";
        writer.write_all(req_json.as_bytes()).await?;

        // Read response
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: SignResponse = serde_json::from_str(line.trim())
            .context("Invalid response from custody-signer")?;

        // Verify response HMAC
        let result_json = serde_json::to_string(&response.result)?;
        if !verify_hmac(&self.ipc_secret, &request_id, &result_json, &response.hmac) {
            anyhow::bail!("Response HMAC verification failed — possible tampering");
        }

        Ok(response.result)
    }

    /// Get a public key / address for a chain and derivation index.
    pub async fn get_public_key(&self, chain: custody_common::Chain, index: u32) -> Result<String> {
        let result = self.request(chain, SignOperation::GetPublicKey {
            derivation_index: index,
        }).await?;

        match result {
            SignResult::Success { signed_data, .. } => Ok(signed_data),
            SignResult::Error { code, message } => {
                anyhow::bail!("Signer error {code}: {message}")
            }
        }
    }

    /// Sign a raw message hash.
    pub async fn raw_sign(
        &self,
        chain: custody_common::Chain,
        index: u32,
        message_hash: &[u8; 32],
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let result = self.request(chain, SignOperation::RawSign {
            derivation_index: index,
            message_hash: hex::encode(message_hash),
        }).await?;

        match result {
            SignResult::Success { signed_data, public_key } => {
                let sig = hex::decode(&signed_data)?;
                let pk = hex::decode(public_key.unwrap_or_default())?;
                Ok((sig, pk))
            }
            SignResult::Error { code, message } => {
                anyhow::bail!("Signer error {code}: {message}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_not_available_without_socket() {
        let client = SignerClient::new("/tmp/nonexistent-socket.sock", b"secret");
        assert!(!client.is_available());
    }

    #[test]
    fn client_creation() {
        let client = SignerClient::new("/tmp/test.sock", b"my_secret");
        assert_eq!(client.socket_path, "/tmp/test.sock");
    }
}
