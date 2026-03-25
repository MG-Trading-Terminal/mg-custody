//! Bitcoin deposit scanner.
//!
//! Scans Bitcoin blocks via Bitcoin Core JSON-RPC for incoming P2WPKH deposits.
//! Uses reqwest for HTTP calls (compatible with any Bitcoin Core node).

use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::ChainScanner;

/// Bitcoin Core JSON-RPC request.
#[derive(Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: String,
    params: serde_json::Value,
}

/// Bitcoin Core JSON-RPC response.
#[derive(Deserialize)]
struct RpcResponse {
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

/// Bitcoin block scanner using Bitcoin Core JSON-RPC.
pub struct BitcoinScanner {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    client: reqwest::Client,
    db: CustodyDb,
    address_cache: AddressCache,
    required_confirmations: u64,
}

impl BitcoinScanner {
    pub fn new(
        rpc_url: &str,
        rpc_user: Option<String>,
        rpc_password: Option<String>,
        db: CustodyDb,
        address_cache: AddressCache,
        required_confirmations: u64,
    ) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rpc_user,
            rpc_password,
            client: reqwest::Client::new(),
            db,
            address_cache,
            required_confirmations,
        }
    }

    /// Make a JSON-RPC call to Bitcoin Core.
    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: method.to_string(),
            params,
        };

        let mut http_req = self.client.post(&self.rpc_url).json(&req);

        if let (Some(user), Some(pass)) = (&self.rpc_user, &self.rpc_password) {
            http_req = http_req.basic_auth(user, Some(pass));
        }

        let resp: RpcResponse = http_req
            .send().await
            .with_context(|| format!("Bitcoin RPC {method} failed"))?
            .json().await
            .context("Bitcoin RPC response parse")?;

        if let Some(err) = resp.error {
            anyhow::bail!("Bitcoin RPC error: {err}");
        }

        resp.result.ok_or_else(|| anyhow::anyhow!("Empty RPC result"))
    }

    /// Scan a single block for deposits to our addresses.
    async fn scan_block(&self, block_height: u64) -> Result<Vec<Deposit>> {
        // getblockhash
        let hash = self.rpc_call("getblockhash", serde_json::json!([block_height])).await?;
        let hash_str = hash.as_str().ok_or_else(|| anyhow::anyhow!("Bad hash"))?;

        // getblock with verbosity=2 (includes full tx data)
        let block = self.rpc_call("getblock", serde_json::json!([hash_str, 2])).await?;

        let empty_arr = vec![];
        let txs = block["tx"].as_array().unwrap_or(&empty_arr);
        let mut deposits = Vec::new();

        for tx in txs {
            let txid = tx["txid"].as_str().unwrap_or("");
            let empty_vout = vec![];
            let vout = tx["vout"].as_array().unwrap_or(&empty_vout);

            for (vout_idx, output) in vout.iter().enumerate() {
                // Check if this output is to one of our addresses
                let addresses = output["scriptPubKey"]["address"].as_str();
                let value_btc = output["value"].as_f64().unwrap_or(0.0);

                if let Some(addr) = addresses {
                    if self.address_cache.contains(addr) {
                        let amount_sats = (value_btc * 100_000_000.0).round() as u64;
                        let amount = Decimal::new(amount_sats as i64, 8);

                        let user_id = self.address_cache
                            .lookup(addr)
                            .unwrap_or(Uuid::nil());

                        deposits.push(Deposit {
                            id: Uuid::new_v4(),
                            user_id,
                            asset: Asset::new("BTC"),
                            chain: Chain::Bitcoin,
                            address: addr.to_string(),
                            amount,
                            tx_hash: txid.to_string(),
                            log_index: Some(vout_idx as u32),
                            block_height,
                            confirmations: 0,
                            required_confirmations: self.required_confirmations,
                            status: DepositStatus::Detected,
                            detected_at: Utc::now(),
                            confirmed_at: None,
                            credited_at: None,
                        });
                    }
                }
            }
        }

        Ok(deposits)
    }
}

impl ChainScanner for BitcoinScanner {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let safe_block = current.saturating_sub(self.required_confirmations);

        if from_block > safe_block {
            return Ok((vec![], from_block));
        }

        let batch_size = 10; // Bitcoin blocks are larger, scan fewer
        let to_block = std::cmp::min(from_block + batch_size - 1, safe_block);

        self.address_cache.refresh_if_stale(current).await?;

        let mut all_deposits = Vec::new();
        for height in from_block..=to_block {
            match self.scan_block(height).await {
                Ok(deposits) => {
                    for deposit in deposits {
                        if self.db.find_deposit_by_tx_hash(&deposit.tx_hash).await?.is_none() {
                            self.db.insert_deposit(&deposit).await?;
                            all_deposits.push(deposit);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(height, error = %e, "Bitcoin block scan failed");
                    return Ok((all_deposits, height));
                }
            }
        }

        tracing::info!(from = from_block, to = to_block, deposits = all_deposits.len(), "Bitcoin scan");
        Ok((all_deposits, to_block + 1))
    }

    async fn current_block_height(&self) -> Result<u64> {
        let result = self.rpc_call("getblockcount", serde_json::json!([])).await?;
        result.as_u64().ok_or_else(|| anyhow::anyhow!("Bad block count"))
    }
}

/// Convert BTC (f64 from RPC) to satoshis safely.
pub fn btc_to_sats(btc: f64) -> u64 {
    (btc * 100_000_000.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btc_to_sats_conversion() {
        assert_eq!(btc_to_sats(1.0), 100_000_000);
        assert_eq!(btc_to_sats(0.5), 50_000_000);
        assert_eq!(btc_to_sats(0.00000001), 1);
        assert_eq!(btc_to_sats(21_000_000.0), 2_100_000_000_000_000);
    }

    #[test]
    fn rpc_request_format() {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getblockcount".into(),
            params: serde_json::json!([]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("getblockcount"));
        assert!(json.contains("2.0"));
    }
}
