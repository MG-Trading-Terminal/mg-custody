//! Solana deposit scanner.
//!
//! Uses Solana JSON-RPC API to detect incoming SOL and SPL token transfers.
//! Scanning approach: getSignaturesForAddress per deposit address.

use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ChainScanner;

/// Solana JSON-RPC request.
#[derive(Serialize)]
struct SolRpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: String,
    params: serde_json::Value,
}

/// Solana RPC response.
#[derive(Deserialize)]
struct SolRpcResponse {
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

/// Solana scanner.
pub struct SolanaScanner {
    rpc_url: String,
    client: reqwest::Client,
    db: CustodyDb,
    deposit_addresses: Vec<String>,
    commitment: String,
}

impl SolanaScanner {
    pub fn new(rpc_url: &str, db: CustodyDb, commitment: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            client: reqwest::Client::new(),
            db,
            deposit_addresses: Vec::new(),
            commitment: commitment.to_string(),
        }
    }

    pub fn add_address(&mut self, address: &str) {
        self.deposit_addresses.push(address.to_string());
    }

    /// Make a Solana JSON-RPC call.
    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let req = SolRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: method.to_string(),
            params,
        };

        let resp: SolRpcResponse = self.client
            .post(&self.rpc_url)
            .json(&req)
            .send().await
            .with_context(|| format!("Solana RPC {method}"))?
            .json().await
            .context("Solana RPC parse")?;

        if let Some(err) = resp.error {
            anyhow::bail!("Solana RPC error: {err}");
        }

        resp.result.ok_or_else(|| anyhow::anyhow!("Empty result"))
    }

    /// Get recent signatures for an address.
    async fn get_signatures(&self, address: &str, limit: u64) -> Result<Vec<serde_json::Value>> {
        let result = self.rpc_call("getSignaturesForAddress", serde_json::json!([
            address,
            {"limit": limit, "commitment": self.commitment}
        ])).await?;

        Ok(result.as_array().cloned().unwrap_or_default())
    }

    /// Get transaction details.
    async fn get_transaction(&self, signature: &str) -> Result<serde_json::Value> {
        self.rpc_call("getTransaction", serde_json::json!([
            signature,
            {"encoding": "json", "commitment": self.commitment, "maxSupportedTransactionVersion": 0}
        ])).await
    }
}

impl ChainScanner for SolanaScanner {
    fn chain(&self) -> Chain {
        Chain::Solana
    }

    async fn scan(&self, _from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let mut deposits = Vec::new();

        // For each monitored address, check recent transactions
        for address in &self.deposit_addresses {
            let sigs = self.get_signatures(address, 20).await?;

            for sig_info in sigs {
                let signature = sig_info["signature"].as_str().unwrap_or("");
                let slot = sig_info["slot"].as_u64().unwrap_or(0);
                let err = &sig_info["err"];

                if !err.is_null() { continue; } // skip failed txs

                // Check if already processed
                if self.db.find_deposit_by_tx_hash(signature).await?.is_some() {
                    continue;
                }

                // Get transaction details for amount
                if let Ok(tx) = self.get_transaction(signature).await {
                    let meta = &tx["meta"];
                    let pre_balances = meta["preBalances"].as_array();
                    let post_balances = meta["postBalances"].as_array();

                    if let (Some(pre), Some(post)) = (pre_balances, post_balances) {
                        // Find the account index for our address
                        let accounts = tx["transaction"]["message"]["accountKeys"]
                            .as_array();

                        if let Some(accts) = accounts {
                            for (i, acct) in accts.iter().enumerate() {
                                let acct_addr = acct.as_str().unwrap_or("");
                                if acct_addr == address {
                                    let pre_bal = pre.get(i).and_then(|v| v.as_u64()).unwrap_or(0);
                                    let post_bal = post.get(i).and_then(|v| v.as_u64()).unwrap_or(0);

                                    if post_bal > pre_bal {
                                        let lamports = post_bal - pre_bal;
                                        let amount = Decimal::new(lamports as i64, 9);

                                        deposits.push(Deposit {
                                            id: Uuid::new_v4(),
                                            user_id: Uuid::nil(),
                                            asset: Asset::new("SOL"),
                                            chain: Chain::Solana,
                                            address: address.clone(),
                                            amount,
                                            tx_hash: signature.to_string(),
                                            log_index: None,
                                            block_height: slot,
                                            confirmations: 0,
                                            required_confirmations: 1,
                                            status: DepositStatus::Detected,
                                            detected_at: Utc::now(),
                                            confirmed_at: None,
                                            credited_at: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Dedup and persist
        for deposit in &deposits {
            self.db.insert_deposit(deposit).await?;
        }

        tracing::info!(addresses = self.deposit_addresses.len(), deposits = deposits.len(), "Solana scan");
        Ok((deposits, current))
    }

    async fn current_block_height(&self) -> Result<u64> {
        let result = self.rpc_call("getSlot", serde_json::json!([
            {"commitment": self.commitment}
        ])).await?;
        result.as_u64().ok_or_else(|| anyhow::anyhow!("Bad slot"))
    }
}

/// Lamports to SOL.
pub fn lamports_to_sol(lamports: u64) -> Decimal {
    Decimal::new(lamports as i64, 9)
}

pub fn sol_to_lamports(sol: Decimal) -> u64 {
    use rust_decimal::prelude::ToPrimitive;
    (sol * Decimal::new(1_000_000_000, 0)).trunc().to_u64().unwrap_or(0)
}

pub const BLOCKHASH_TTL_SECS: u64 = 90;
pub const MAX_BLOCKHASH_RETRIES: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn lamports_sol_conversion() {
        assert_eq!(lamports_to_sol(1_000_000_000), dec!(1.000000000));
        assert_eq!(lamports_to_sol(500_000_000), dec!(0.500000000));
        assert_eq!(sol_to_lamports(dec!(1.0)), 1_000_000_000);
    }

    #[test]
    fn rpc_request_format() {
        let req = SolRpcRequest {
            jsonrpc: "2.0", id: 1,
            method: "getSlot".into(),
            params: serde_json::json!([{"commitment": "finalized"}]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("getSlot"));
        assert!(json.contains("finalized"));
    }
}
