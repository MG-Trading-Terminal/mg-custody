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
        let sig_result = self.rpc("sendTransaction", serde_json::json!([
            encoded,
            {"encoding": "base58", "skipPreflight": false, "preflightCommitment": self.commitment}
        ])).await;

        // Detect blockhash expiry loudly. A tx signed more than
        // ~90 seconds before submission lands with an expired
        // blockhash, the RPC rejects it, and without the loud log
        // the operator sees a generic "Solana RPC error" in logs
        // that doesn't point at "re-sign with a fresh blockhash".
        // `is_blockhash_expired` was already defined but never
        // called — finally wiring it up.
        if let Err(ref e) = sig_result
            && is_blockhash_expired(&e.to_string())
        {
            tracing::error!(
                error = %e,
                "Solana broadcast rejected due to expired blockhash — \
                 caller must re-sign with getLatestBlockhash() and retry. \
                 A signed tx has a ~90s TTL; anything slower than that \
                 needs to be built again, not resubmitted."
            );
        }

        let sig = sig_result?;
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

        // Same class of bug as the ETH path: if `slot` is
        // missing from a non-null getTransaction result, that's
        // a malformed RPC payload and we must NOT treat it as
        // `slot = 0` (which would make confirmations = current -
        // 0 = massive, triggering premature finalization and a
        // withdrawal_confirmed webhook for a tx whose actual
        // position we don't know). Treat as "not yet visible";
        // the next tick will retry when the RPC returns a
        // complete response.
        let Some(slot) = result["slot"].as_u64() else {
            tracing::warn!(
                tx = %tx_hash,
                "Solana getTransaction returned result without `slot` — \
                 treating as not yet visible rather than defaulting to 0"
            );
            return Ok(None);
        };
        let err = &result["meta"]["err"];
        let success = err.is_null();

        // Current slot lookup must also be honest. A failing
        // getSlot used to default to `current = 0`, which made
        // `confirmations = saturating_sub(0, slot) = 0` — the
        // withdrawal stayed in Confirming forever even after
        // it had confirmed, because we pretended the chain had
        // rewound to slot 0. Propagate the error so the
        // pipeline retries on the next tick.
        let current_raw = self
            .rpc(
                "getSlot",
                serde_json::json!([{"commitment": self.commitment}]),
            )
            .await?;
        let current = current_raw.as_u64().ok_or_else(|| {
            anyhow::anyhow!(
                "Solana getSlot returned non-numeric value: {current_raw} — \
                 cannot compute confirmations"
            )
        })?;

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
