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

/// SPL token to scan for.
#[derive(Debug, Clone)]
pub struct SplToken {
    pub symbol: String,
    pub mint: String,
    pub decimals: u8,
}

/// Solana scanner (SOL native + SPL tokens).
pub struct SolanaScanner {
    rpc_url: String,
    client: reqwest::Client,
    db: CustodyDb,
    deposit_addresses: Vec<String>,
    commitment: String,
    /// SPL tokens to detect (USDT, USDC, etc.)
    spl_tokens: Vec<SplToken>,
}

impl SolanaScanner {
    pub fn new(rpc_url: &str, db: CustodyDb, commitment: &str) -> Self {
        // Default SPL tokens
        let spl_tokens = vec![
            SplToken {
                symbol: "USDT".into(),
                mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".into(),
                decimals: 6,
            },
            SplToken {
                symbol: "USDC".into(),
                mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
                decimals: 6,
            },
        ];

        Self {
            rpc_url: rpc_url.to_string(),
            client: reqwest::Client::new(),
            db,
            deposit_addresses: Vec::new(),
            commitment: commitment.to_string(),
            spl_tokens,
        }
    }

    pub fn add_address(&mut self, address: &str) {
        self.deposit_addresses.push(address.to_string());
    }

    pub fn add_spl_token(&mut self, symbol: &str, mint: &str, decimals: u8) {
        self.spl_tokens.push(SplToken { symbol: symbol.into(), mint: mint.into(), decimals });
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

                    // ── SOL native detection ──────────────────
                    let pre_balances = meta["preBalances"].as_array();
                    let post_balances = meta["postBalances"].as_array();

                    if let (Some(pre), Some(post)) = (pre_balances, post_balances) {
                        let accounts = tx["transaction"]["message"]["accountKeys"].as_array();

                        if let Some(accts) = accounts {
                            for (i, acct) in accts.iter().enumerate() {
                                let acct_key = acct.as_str()
                                    .or_else(|| acct["pubkey"].as_str())
                                    .unwrap_or("");
                                if acct_key == address {
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
                                            memo: None,
                                            source_memo: None,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // ── SPL token detection ───────────────────
                    // postTokenBalances contains token account states after tx.
                    // Compare with preTokenBalances to find incoming transfers.
                    if !self.spl_tokens.is_empty() {
                        let pre_token = meta["preTokenBalances"].as_array();
                        let post_token = meta["postTokenBalances"].as_array();

                        if let Some(post_tokens) = post_token {
                            let pre_tokens = pre_token.cloned().unwrap_or_default();

                            for post_entry in post_tokens {
                                let mint = post_entry["mint"].as_str().unwrap_or("");
                                let owner = post_entry["owner"].as_str().unwrap_or("");
                                let account_index = post_entry["accountIndex"].as_u64().unwrap_or(u64::MAX);

                                // Check if this token owner is our deposit address
                                if owner != address { continue; }

                                // Match against our SPL tokens
                                let Some(token) = self.spl_tokens.iter().find(|t| t.mint == mint) else { continue; };

                                let post_amount_str = post_entry["uiTokenAmount"]["amount"].as_str().unwrap_or("0");
                                let post_amount: u128 = post_amount_str.parse().unwrap_or(0);

                                // Find pre-balance for same account index
                                let pre_amount: u128 = pre_tokens.iter()
                                    .find(|e| e["accountIndex"].as_u64() == Some(account_index))
                                    .and_then(|e| e["uiTokenAmount"]["amount"].as_str())
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0);

                                if post_amount > pre_amount {
                                    let received = post_amount - pre_amount;
                                    let amount = Decimal::new(received as i64, token.decimals.into());

                                    deposits.push(Deposit {
                                        id: Uuid::new_v4(),
                                        user_id: Uuid::nil(),
                                        asset: Asset::new(&token.symbol),
                                        chain: Chain::Solana,
                                        address: address.clone(),
                                        amount,
                                        tx_hash: signature.to_string(),
                                        log_index: Some(account_index as u32),
                                        block_height: slot,
                                        confirmations: 0,
                                        required_confirmations: 1,
                                        status: DepositStatus::Detected,
                                        detected_at: Utc::now(),
                                        confirmed_at: None,
                                        credited_at: None,
                                        memo: None,
                                        source_memo: None,
                                    });

                                    tracing::info!(
                                        tx = signature, token = %token.symbol, amount = %amount,
                                        owner = owner, mint = mint, slot = slot,
                                        "SPL token deposit detected"
                                    );
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
    fn spl_token_config() {
        let usdt = SplToken {
            symbol: "USDT".into(),
            mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".into(),
            decimals: 6,
        };
        assert_eq!(usdt.symbol, "USDT");
        assert_eq!(usdt.decimals, 6);
        assert_eq!(usdt.mint.len(), 44);

        let usdc = SplToken {
            symbol: "USDC".into(),
            mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
            decimals: 6,
        };
        assert_ne!(usdt.mint, usdc.mint);
    }

    #[test]
    fn spl_amount_parsing() {
        // 1 USDT = 1,000,000 base units (6 decimals)
        let amount_str = "1000000";
        let amount: u128 = amount_str.parse().unwrap();
        let decimal = Decimal::new(amount as i64, 6);
        assert_eq!(decimal, dec!(1.000000));

        // 50,000 USDC
        let amount: u128 = 50_000_000_000;
        let decimal = Decimal::new(amount as i64, 6);
        assert_eq!(decimal, dec!(50000.000000));
    }

    #[test]
    fn spl_balance_diff_detection() {
        // Simulate: pre=0, post=5000000 → received 5 USDT
        let pre: u128 = 0;
        let post: u128 = 5_000_000;
        assert!(post > pre);
        let received = post - pre;
        assert_eq!(received, 5_000_000);
        let amount = Decimal::new(received as i64, 6);
        assert_eq!(amount, dec!(5.000000));
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
