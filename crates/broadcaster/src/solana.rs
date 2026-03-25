//! Solana transaction broadcaster via JSON-RPC.

use anyhow::{Context, Result};
use custody_common::Chain;
use crate::{Broadcaster, TxStatus};

/// Maximum retries with fresh blockhash.
pub const MAX_BLOCKHASH_RETRIES: u32 = 3;

pub struct SolanaBroadcaster {
    rpc_url: String,
    client: reqwest::Client,
    commitment: String,
}

impl SolanaBroadcaster {
    pub fn new(rpc_url: &str, commitment: &str) -> Self {
        Self { rpc_url: rpc_url.to_string(), client: reqwest::Client::new(), commitment: commitment.to_string() }
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp: serde_json::Value = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send().await.context("Solana RPC")?
            .json().await?;
        if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
            anyhow::bail!("Solana RPC error: {err}");
        }
        Ok(resp["result"].clone())
    }
}

impl Broadcaster for SolanaBroadcaster {
    fn chain(&self) -> Chain { Chain::Solana }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        let encoded = bs58::encode(signed_tx).into_string();
        let sig = self.rpc("sendTransaction", serde_json::json!([
            encoded,
            {"encoding": "base58", "skipPreflight": false, "preflightCommitment": self.commitment}
        ])).await?;
        let hash = sig.as_str().unwrap_or("").to_string();
        tracing::info!(sig = %hash, "Solana tx broadcast");
        Ok(hash)
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        let result = self.rpc("getTransaction", serde_json::json!([
            tx_hash,
            {"encoding": "json", "commitment": self.commitment}
        ])).await?;

        if result.is_null() {
            return Ok(None); // not yet confirmed
        }

        let slot = result["slot"].as_u64().unwrap_or(0);
        let err = &result["meta"]["err"];
        let success = err.is_null();

        // Get current slot for confirmation count
        let current = self.rpc("getSlot", serde_json::json!([{"commitment": self.commitment}])).await?
            .as_u64().unwrap_or(0);

        Ok(Some(TxStatus {
            block_number: slot,
            confirmations: current.saturating_sub(slot),
            success,
        }))
    }
}

/// Check if an error indicates a blockhash expiry.
pub fn is_blockhash_expired(error_msg: &str) -> bool {
    error_msg.contains("BlockhashNotFound") || error_msg.contains("blockhash not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockhash_expiry_detection() {
        assert!(is_blockhash_expired("BlockhashNotFound"));
        assert!(!is_blockhash_expired("InsufficientFunds"));
    }
}
