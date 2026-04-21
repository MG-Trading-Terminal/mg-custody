//! Bitcoin transaction broadcaster via Bitcoin Core JSON-RPC.

use anyhow::{Context, Result};
use custody_common::Chain;
use crate::{Broadcaster, TxStatus};

pub struct BitcoinBroadcaster {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    client: reqwest::Client,
}

impl BitcoinBroadcaster {
    pub fn new(rpc_url: &str, rpc_user: Option<String>, rpc_password: Option<String>) -> Self {
        Self { rpc_url: rpc_url.to_string(), rpc_user, rpc_password, client: reqwest::Client::new() }
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let mut req = self.client.post(&self.rpc_url).json(&body);
        if let (Some(u), Some(p)) = (&self.rpc_user, &self.rpc_password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp: serde_json::Value = req.send().await.context("BTC RPC")?.json().await?;
        if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
            anyhow::bail!("BTC RPC error: {err}");
        }
        Ok(resp["result"].clone())
    }
}

impl Broadcaster for BitcoinBroadcaster {
    fn chain(&self) -> Chain { Chain::Bitcoin }

    async fn broadcast(&self, signed_tx: &[u8]) -> Result<String> {
        let tx_hex = hex::encode(signed_tx);
        let txid = self.rpc("sendrawtransaction", serde_json::json!([tx_hex])).await?;
        // Refuse to return an empty txid. `sendrawtransaction`
        // succeeding means the tx was accepted into the mempool —
        // money is committed. If the response shape is wrong
        // (not a string), the previous `unwrap_or("")` would
        // hand an empty hash back to the pipeline, which stored
        // it as the withdrawal's tx_hash. Downstream tx_status
        // calls then polled an empty string forever, webhooks
        // fired `tx_hash: ""`, and the confirmation tracker
        // looped on a row that could never finalise. Bail with
        // the raw response so the operator can recover the real
        // txid from node logs and repair the row manually.
        let hash = txid.as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Bitcoin sendrawtransaction returned non-string result: {txid} — \
                 tx was broadcast but txid was lost; operator must recover \
                 from node logs"
            )
        })?.to_string();
        tracing::info!(txid = %hash, "Bitcoin tx broadcast");
        Ok(hash)
    }

    async fn tx_status(&self, tx_hash: &str) -> Result<Option<TxStatus>> {
        let tx = self.rpc("getrawtransaction", serde_json::json!([tx_hash, true])).await;
        match tx {
            Ok(tx) => {
                let confirmations = tx["confirmations"].as_u64().unwrap_or(0);
                let block_height = tx["blockheight"].as_u64().unwrap_or(0);
                Ok(Some(TxStatus { block_number: block_height, confirmations, success: true }))
            }
            Err(_) => Ok(None),
        }
    }
}
