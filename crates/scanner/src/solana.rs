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
    /// Address → user_id map. Populated by `reload_from_db` at
    /// startup and periodically thereafter so new deposit
    /// addresses created via the API become scannable without a
    /// restart. Replaces a prior `Vec<String>` that left
    /// `user_id: Uuid::nil()` on every detected deposit.
    deposit_addresses: tokio::sync::RwLock<std::collections::HashMap<String, Uuid>>,
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
            deposit_addresses: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            commitment: commitment.to_string(),
            spl_tokens,
        }
    }

    /// Confirmations to require before crediting a deposit, driven
    /// by the configured commitment level.
    ///
    /// Previously hardcoded to 1. That was defensible only under
    /// `finalized` commitment (where Solana itself requires 32+
    /// slots + supermajority stake). Under `confirmed` (~400ms-2s
    /// of consensus) a single confirmation is NOT final — a reorg
    /// in the first few seconds can replay the deposit. Under
    /// `processed` (one leader's local view) it's nowhere near
    /// final. This helper rescales per commitment.
    fn required_confirmations(&self) -> u64 {
        match self.commitment.as_str() {
            "finalized" => 1,
            // "confirmed" is supermajority-stake but not root;
            // pair it with 32 additional slots to get protocol-
            // equivalent finality before crediting.
            "confirmed" => 32,
            // "processed" is leader-only. Double the finalized
            // threshold as a conservative floor; still insufficient
            // for high-value deposits — operators should run
            // "finalized" in prod.
            _ => 64,
        }
    }

    /// Manually register an address (for tests + ad-hoc flows).
    /// Production callers should use `reload_from_db` so the cache
    /// stays in sync with the `addresses` table.
    pub async fn add_address(&self, address: &str, user_id: Uuid) {
        self.deposit_addresses
            .write()
            .await
            .insert(address.to_string(), user_id);
    }

    /// Pull every known Solana deposit address from the DB into
    /// the in-memory map. Call this at startup and periodically
    /// (every scan tick is fine — the query is cheap and the map
    /// is small) so new user-generated addresses get picked up
    /// without a restart.
    ///
    /// Silently does nothing on DB error; the scanner keeps its
    /// previous snapshot rather than wiping the cache mid-flight.
    pub async fn reload_from_db(&self) -> Result<()> {
        let pairs = self
            .db
            .addresses_with_users(Chain::Solana)
            .await
            .context("reload Solana addresses")?;
        let mut map = self.deposit_addresses.write().await;
        // Full rebuild — cheaper than reconciling diffs for the
        // typical cache size (hundreds to low thousands of
        // addresses).
        map.clear();
        for (addr, uid) in pairs {
            map.insert(addr, uid);
        }
        tracing::debug!(
            count = map.len(),
            "Solana address map reloaded from DB"
        );
        Ok(())
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

/// Solana's official memo programs. A transaction may carry at
/// most a handful of memo instructions (Solana doesn't have a
/// memo field like Stellar/XRP; you attach a Memo Program
/// instruction instead). We accept both the v1 no-signer program
/// and the v2 program that optionally requires a signer.
const MEMO_PROGRAM_V1: &str = "Memo1111111111111111111111111111111111111111";
const MEMO_PROGRAM_V2: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

/// Walk the `transaction.message.instructions` array looking for
/// Memo-Program instructions and concatenate their decoded data
/// with "; " separators. Previously every deposit's `source_memo`
/// was hardcoded to `None`, so exchange integrations that match
/// deposits by memo (e.g. a deposit address shared across users,
/// disambiguated by memo) had no data to work with.
///
/// Base58 decode of the instruction `data` field yields the raw
/// bytes the user embedded; we assume UTF-8 per Memo Program
/// convention and lossy-decode rather than error — a non-UTF-8
/// memo is still better surfaced as replacement characters than
/// silently dropped.
fn extract_solana_memo(tx: &serde_json::Value) -> Option<String> {
    let accounts = tx["transaction"]["message"]["accountKeys"].as_array()?;
    let instructions = tx["transaction"]["message"]["instructions"].as_array()?;
    let mut memos: Vec<String> = Vec::new();
    for ix in instructions {
        let program_id_idx = ix.get("programIdIndex").and_then(|v| v.as_u64());
        let Some(idx) = program_id_idx else { continue };
        let program_id = accounts
            .get(idx as usize)
            .and_then(|v| v.as_str().or_else(|| v["pubkey"].as_str()))
            .unwrap_or("");
        if program_id != MEMO_PROGRAM_V1 && program_id != MEMO_PROGRAM_V2 {
            continue;
        }
        let data_b58 = match ix.get("data").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        match bs58::decode(data_b58).into_vec() {
            Ok(bytes) => {
                let s = String::from_utf8_lossy(&bytes).to_string();
                if !s.is_empty() {
                    memos.push(s);
                }
            }
            Err(e) => {
                tracing::debug!(
                    data = data_b58, error = %e,
                    "Memo instruction data not valid base58 — skipping"
                );
            }
        }
    }
    if memos.is_empty() {
        None
    } else {
        Some(memos.join("; "))
    }
}

impl ChainScanner for SolanaScanner {
    fn chain(&self) -> Chain {
        Chain::Solana
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let mut deposits = Vec::new();

        // Snapshot the address→user_id map once so we don't hold
        // the RwLock across RPC calls. Clone is cheap (hundreds of
        // entries tops) and avoids deadlocks if reload_from_db
        // fires mid-scan.
        let addresses: Vec<(String, Uuid)> = self
            .deposit_addresses
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        for (address, user_id) in &addresses {
            let address = address.as_str();
            let user_id = *user_id;
            let sigs = self.get_signatures(address, 100).await?;

            for sig_info in sigs {
                let signature = sig_info["signature"].as_str().unwrap_or("");
                let slot = sig_info["slot"].as_u64().unwrap_or(0);
                let err = &sig_info["err"];

                if !err.is_null() { continue; } // skip failed txs

                // Honor from_block so the scanner doesn't reprocess
                // the same window forever after a restart. Previous
                // code ignored `from_block` entirely (renamed it to
                // `_from_block`) and relied on `find_deposit_by_tx_hash`
                // for dedup — which works but also silently hid
                // continuity gaps: if the backfill window didn't
                // reach a missed slot, nothing surfaced it. Now we
                // only consider signatures at slot >= from_block.
                if slot < from_block {
                    continue;
                }

                // Check if already processed
                if self.db.find_deposit_by_tx_hash(signature).await?.is_some() {
                    continue;
                }

                // Get transaction details for amount
                if let Ok(tx) = self.get_transaction(signature).await {
                    let meta = &tx["meta"];
                    // Extract memo once per tx. Both SOL and SPL
                    // deposit constructors below reuse this —
                    // previously `source_memo: None` was hardcoded
                    // so exchange integrations that match deposits
                    // via Solana Memo Program had no signal.
                    let source_memo = extract_solana_memo(&tx);

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
                                            user_id,
                                            asset: Asset::new("SOL"),
                                            chain: Chain::Solana,
                                            address: address.to_string(),
                                            amount,
                                            tx_hash: signature.to_string(),
                                            log_index: None,
                                            block_height: slot,
                                            confirmations: 0,
                                            required_confirmations: self.required_confirmations(),
                                            status: DepositStatus::Detected,
                                            detected_at: Utc::now(),
                                            confirmed_at: None,
                                            credited_at: None,
                                            memo: None,
                                            source_memo: source_memo.clone(),
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
                                let post_amount: u128 = match post_amount_str.parse() {
                                    Ok(n) => n,
                                    Err(e) => {
                                        // Silent `.unwrap_or(0)` here
                                        // used to drop the deposit
                                        // entirely on any parse
                                        // error — a malformed RPC
                                        // payload looked identical to
                                        // "no transfer happened". Now
                                        // we log and skip this deposit
                                        // so the gap is visible.
                                        tracing::warn!(
                                            raw = %post_amount_str,
                                            error = %e,
                                            mint,
                                            owner,
                                            signature,
                                            "Solana SPL post-balance amount unparseable — \
                                             skipping this deposit"
                                        );
                                        continue;
                                    }
                                };

                                // Find pre-balance for same account index.
                                // `.ok()` on the parse step falls back to 0
                                // which is the right default for "no pre
                                // balance row existed yet" (a freshly-
                                // created token account).
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
                                        user_id,
                                        asset: Asset::new(&token.symbol),
                                        chain: Chain::Solana,
                                        address: address.to_string(),
                                        amount,
                                        tx_hash: signature.to_string(),
                                        log_index: Some(account_index as u32),
                                        block_height: slot,
                                        confirmations: 0,
                                        required_confirmations: self.required_confirmations(),
                                        status: DepositStatus::Detected,
                                        detected_at: Utc::now(),
                                        confirmed_at: None,
                                        credited_at: None,
                                        memo: None,
                                        source_memo: source_memo.clone(),
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

        tracing::info!(addresses = addresses.len(), deposits = deposits.len(), "Solana scan");
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
    fn extract_memo_picks_up_v2_program_data() {
        // Build a minimal getTransaction response shape with one
        // memo-program instruction. "Hello" base58 = "9Ajdvzr".
        let memo_text = "trace-12345";
        let memo_b58 = bs58::encode(memo_text.as_bytes()).into_string();
        let tx = serde_json::json!({
            "transaction": {
                "message": {
                    "accountKeys": [
                        "SenderPubkey1111111111111111111111111111111",
                        MEMO_PROGRAM_V2,
                    ],
                    "instructions": [
                        {"programIdIndex": 1, "data": memo_b58, "accounts": []},
                    ],
                }
            }
        });
        assert_eq!(extract_solana_memo(&tx), Some(memo_text.to_string()));
    }

    #[test]
    fn extract_memo_ignores_non_memo_programs() {
        let tx = serde_json::json!({
            "transaction": {
                "message": {
                    "accountKeys": ["Sender", "SystemProgram"],
                    "instructions": [
                        {"programIdIndex": 1, "data": "abcdef", "accounts": []},
                    ],
                }
            }
        });
        assert_eq!(extract_solana_memo(&tx), None);
    }

    #[test]
    fn extract_memo_joins_multiple_with_semicolon() {
        let a = bs58::encode(b"first").into_string();
        let b = bs58::encode(b"second").into_string();
        let tx = serde_json::json!({
            "transaction": {
                "message": {
                    "accountKeys": [MEMO_PROGRAM_V1, "_"],
                    "instructions": [
                        {"programIdIndex": 0, "data": a},
                        {"programIdIndex": 0, "data": b},
                    ],
                }
            }
        });
        assert_eq!(extract_solana_memo(&tx), Some("first; second".to_string()));
    }

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
