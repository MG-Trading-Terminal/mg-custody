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
            let receipt_status = resp["receipt"]["result"].as_str().unwrap_or("SUCCESS");
            Ok(Some(TxStatus {
                block_number: block,
                confirmations: 0, // need to compute from current block
                success: receipt_status == "SUCCESS",
            }))
        } else {
            Ok(None)
        }
    }
}
