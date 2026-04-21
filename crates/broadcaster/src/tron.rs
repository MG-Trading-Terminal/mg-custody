//! Tron transaction broadcaster via HTTP API.

use anyhow::{Context, Result};
use custody_common::Chain;
use crate::{Broadcaster, TxStatus};

pub struct TronBroadcaster {
    api_url: String,
    client: reqwest::Client,
}

impl TronBroadcaster {
    pub fn new(api_url: &str) -> Self {
        Self { api_url: api_url.to_string(), client: reqwest::Client::new() }
    }
}

impl Broadcaster for TronBroadcaster {
    fn chain(&self) -> Chain { Chain::Tron }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        let tx_json: serde_json::Value = serde_json::from_slice(signed_tx)
            .context("Tron signed tx should be JSON")?;

        let resp: serde_json::Value = self.client
            .post(format!("{}/wallet/broadcasttransaction", self.api_url))
            .json(&tx_json)
            .send().await.context("Tron broadcast")?
            .json().await?;

        if resp["result"].as_bool() == Some(true) {
            let txid = resp["txid"].as_str().unwrap_or("").to_string();
            tracing::info!(txid = %txid, "Tron tx broadcast");
            Ok(txid)
        } else {
            anyhow::bail!("Tron broadcast failed: {}", resp)
        }
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        let resp: serde_json::Value = self.client
            .post(format!("{}/wallet/gettransactioninfobyid", self.api_url))
            .json(&serde_json::json!({"value": tx_hash}))
            .send().await?
            .json().await?;

        if resp["blockNumber"].is_number() {
            let block = resp["blockNumber"].as_u64().unwrap_or(0);
            // Tron receipt.result semantics:
            //   "SUCCESS"       — tx executed OK
            //   "REVERT"        — EVM/contract revert
            //   "OUT_OF_ENERGY" — ran out of Energy mid-execution
            //   "BAD_JUMP_DESTINATION" / others — contract bugs
            //   null / missing  — tx not yet processed; treat as
            //                     pending rather than defaulting
            //                     to SUCCESS (previous code's
            //                     behaviour, which masked genuine
            //                     contract failures until someone
            //                     went looking)
            let receipt_result = resp["receipt"]["result"].as_str();
            let success = match receipt_result {
                Some("SUCCESS") => true,
                Some(other) => {
                    tracing::warn!(
                        txid = %tx_hash,
                        result = %other,
                        "Tron tx failed on-chain (receipt.result != SUCCESS)"
                    );
                    false
                }
                None => {
                    // Block assigned but no receipt yet — rare race
                    // window between inclusion and receipt-indexing
                    // on the node. Return pending so the caller
                    // keeps polling rather than prematurely marking
                    // success.
                    return Ok(None);
                }
            };

            // Confirmations = current head - tx block. Previously
            // hardcoded 0 with a "need to compute" comment, which
            // meant the confirmation tracker polled forever and
            // clients saw every Tron tx as perpetually pending.
            let confirmations = match self.current_block_height().await {
                Ok(head) if head >= block => head - block,
                Ok(_) => 0, // tx block ahead of reported head (race); treat as pending
                Err(e) => {
                    tracing::warn!(
                        txid = %tx_hash, block, error = %e,
                        "Tron getnowblock failed during confirmation compute — \
                         reporting 0 confirmations for this tick"
                    );
                    0
                }
            };

            Ok(Some(TxStatus {
                block_number: block,
                confirmations,
                success,
            }))
        } else {
            Ok(None)
        }
    }
}

impl TronBroadcaster {
    /// Fetch the current chain head block number via
    /// `/wallet/getnowblock`. Separate from `tx_status` so a
    /// failed head lookup doesn't lose the tx-receipt data.
    async fn current_block_height(&self) -> Result<u64> {
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/wallet/getnowblock", self.api_url))
            .json(&serde_json::json!({}))
            .send()
            .await?
            .json()
            .await?;
        let block = resp["block_header"]["raw_data"]["number"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("getnowblock missing block_header.raw_data.number"))?;
        Ok(block)
    }
}
