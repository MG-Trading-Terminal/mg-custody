//! Tron deposit scanner.
//!
//! Scans Tron blocks via HTTP API for TRX native transfers and TRC-20 events.

use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::ChainScanner;

/// TRC-20 USDT mainnet contract address.
pub const TRC20_USDT_CONTRACT: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

/// TRC-20 Transfer event topic = keccak256("Transfer(address,address,uint256)")
/// Same as ERC-20: ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
const TRANSFER_TOPIC: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// Configured TRC-20 token for scanning.
#[derive(Debug, Clone)]
pub struct Trc20Token {
    pub symbol: String,
    pub contract_address: String,  // Base58Check (T...)
    pub contract_hex: String,      // Hex with 41 prefix
    pub decimals: u8,
}

/// Tron block scanner using HTTP API.
pub struct TronScanner {
    api_url: String,
    client: reqwest::Client,
    db: CustodyDb,
    address_cache: AddressCache,
    required_confirmations: u64,
    /// TRC-20 tokens to scan for (USDT, USDC, etc.)
    trc20_tokens: Vec<Trc20Token>,
}

impl TronScanner {
    pub fn new(api_url: &str, db: CustodyDb, address_cache: AddressCache, required_confirmations: u64) -> Self {
        // Default TRC-20 tokens: USDT + USDC
        let trc20_tokens = vec![
            Trc20Token {
                symbol: "USDT".into(),
                contract_address: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".into(),
                contract_hex: "41a614f803b6fd780986a42c78ec9c7f77e6ded13c".into(),
                decimals: 6,
            },
            Trc20Token {
                symbol: "USDC".into(),
                contract_address: "TEkxiTehnzSmSe2XqrBj4w32RUN966rdz8".into(),
                contract_hex: "413487b63d30b5b2c87fb7ffa8bcfade38eaac1abe".into(),
                decimals: 6,
            },
        ];

        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
            db, address_cache, required_confirmations,
            trc20_tokens,
        }
    }

    /// Add a custom TRC-20 token to scan.
    pub fn add_trc20_token(&mut self, symbol: &str, contract_base58: &str, contract_hex: &str, decimals: u8) {
        self.trc20_tokens.push(Trc20Token {
            symbol: symbol.into(),
            contract_address: contract_base58.into(),
            contract_hex: contract_hex.to_lowercase(),
            decimals,
        });
    }

    /// Get a block by number.
    async fn get_block(&self, num: u64) -> Result<serde_json::Value> {
        self.client
            .post(format!("{}/wallet/getblockbynum", self.api_url))
            .json(&serde_json::json!({"num": num}))
            .send().await.context("Tron getblockbynum")?
            .json().await.context("Tron block parse")
    }

    /// Get transaction info (including event logs) for a block.
    /// Uses `gettransactioninfobyblocknum` which returns receipts with logs.
    async fn get_tx_info_by_block(&self, num: u64) -> Result<Vec<serde_json::Value>> {
        let resp: serde_json::Value = self.client
            .post(format!("{}/wallet/gettransactioninfobyblocknum", self.api_url))
            .json(&serde_json::json!({"num": num}))
            .send().await.context("Tron gettransactioninfobyblocknum")?
            .json().await.context("Tron txinfo parse")?;

        Ok(resp.as_array().cloned().unwrap_or_default())
    }

    /// Parse a Tron hex address (41-prefixed, 42 hex chars) to Base58Check.
    pub fn hex_to_base58(hex_addr: &str) -> Option<String> {
        let bytes = hex::decode(hex_addr).ok()?;
        if bytes.len() != 21 || bytes[0] != 0x41 { return None; }
        // Base58Check: payload + 4-byte SHA256d checksum
        use sha2::{Sha256, Digest};
        let hash1 = Sha256::digest(&bytes);
        let hash2 = Sha256::digest(&hash1);
        let mut with_check = bytes.to_vec();
        with_check.extend_from_slice(&hash2[..4]);
        Some(bs58::encode(&with_check).into_string())
    }

    /// Scan a block for TRX native + TRC-20 token transfers to our addresses.
    async fn scan_block(&self, block_num: u64) -> Result<Vec<Deposit>> {
        let block = self.get_block(block_num).await?;
        let empty = vec![];
        let txs = block["transactions"].as_array().unwrap_or(&empty);
        let mut deposits = Vec::new();

        // ── Pass 1: TRX native transfers ─────────────────────────
        for tx in txs {
            let txid = tx["txID"].as_str().unwrap_or("");
            let ret = &tx["ret"];
            let success = ret.as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["contractRet"].as_str())
                .map(|s| s == "SUCCESS")
                .unwrap_or(false);

            if !success { continue; }

            let contracts = tx["raw_data"]["contract"].as_array().unwrap_or(&empty);
            for contract in contracts {
                let ctype = contract["type"].as_str().unwrap_or("");
                let params = &contract["parameter"]["value"];

                if ctype == "TransferContract" {
                    let to = params["to_address"].as_str().unwrap_or("");
                    let amount_sun = params["amount"].as_u64().unwrap_or(0);

                    if self.address_cache.contains(to) && amount_sun > 0 {
                        deposits.push(Deposit {
                            id: Uuid::new_v4(),
                            user_id: self.address_cache.lookup(to).unwrap_or(Uuid::nil()),
                            asset: Asset::new("TRX"),
                            chain: Chain::Tron,
                            address: to.to_string(),
                            amount: sun_to_trx(amount_sun),
                            tx_hash: txid.to_string(),
                            log_index: None,
                            block_height: block_num,
                            confirmations: 0,
                            required_confirmations: self.required_confirmations,
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

        // ── Pass 2: TRC-20 token transfers (event logs) ──────────
        if !self.trc20_tokens.is_empty() {
            let tx_infos = self.get_tx_info_by_block(block_num).await.unwrap_or_default();

            for tx_info in &tx_infos {
                let txid = tx_info["id"].as_str().unwrap_or("");
                let receipt_result = tx_info["receipt"]["result"].as_str().unwrap_or("");
                if receipt_result != "SUCCESS" && !receipt_result.is_empty() {
                    continue; // Skip failed transactions (empty result = success for some TXs)
                }

                let logs = tx_info["log"].as_array().unwrap_or(&empty);
                for (log_idx, log) in logs.iter().enumerate() {
                    // Check if this is a Transfer event
                    let topics = log["topics"].as_array().unwrap_or(&empty);
                    if topics.is_empty() { continue; }

                    let topic0 = topics[0].as_str().unwrap_or("");
                    if topic0 != TRANSFER_TOPIC { continue; }
                    if topics.len() < 3 { continue; } // Transfer has 3 topics

                    // Extract contract address (from the log entry)
                    let contract_hex = format!("41{}", log["address"].as_str().unwrap_or(""));

                    // Match against our configured TRC-20 tokens
                    let token = self.trc20_tokens.iter().find(|t| {
                        t.contract_hex.eq_ignore_ascii_case(&contract_hex)
                    });
                    let Some(token) = token else { continue; };

                    // topics[1] = from (32-byte padded address)
                    // topics[2] = to (32-byte padded address)
                    let to_topic = topics[2].as_str().unwrap_or("");
                    // Last 40 hex chars = address without 41 prefix
                    let to_hex = if to_topic.len() >= 40 {
                        format!("41{}", &to_topic[to_topic.len() - 40..])
                    } else {
                        continue;
                    };

                    // Convert hex to Base58Check
                    let Some(to_base58) = Self::hex_to_base58(&to_hex) else { continue; };

                    // Check if destination is one of our addresses
                    let Some(user_id) = self.address_cache.lookup(&to_base58) else { continue; };

                    // Parse amount from log data
                    let data_hex = log["data"].as_str().unwrap_or("0");
                    let amount_raw = u128::from_str_radix(data_hex.trim_start_matches("0x"), 16).unwrap_or(0);
                    if amount_raw == 0 { continue; }

                    let amount = Decimal::new(amount_raw as i64, token.decimals.into());

                    let addr_str = to_base58.clone();
                    deposits.push(Deposit {
                        id: Uuid::new_v4(),
                        user_id,
                        asset: Asset::new(&token.symbol),
                        chain: Chain::Tron,
                        address: addr_str,
                        amount,
                        tx_hash: txid.to_string(),
                        log_index: Some(log_idx as u32),
                        block_height: block_num,
                        confirmations: 0,
                        required_confirmations: self.required_confirmations,
                        status: DepositStatus::Detected,
                        detected_at: Utc::now(),
                        confirmed_at: None,
                        credited_at: None,
                        memo: None,
                        source_memo: None,
                    });

                    tracing::info!(
                        tx = txid, token = %token.symbol, amount = %amount,
                        to = %to_base58, block = block_num,
                        "TRC-20 deposit detected"
                    );
                }
            }
        }

        Ok(deposits)
    }
}

impl ChainScanner for TronScanner {
    fn chain(&self) -> Chain { Chain::Tron }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        let current = self.current_block_height().await?;
        let safe = current.saturating_sub(self.required_confirmations);
        if from_block > safe { return Ok((vec![], from_block)); }

        let batch = 20;
        let to_block = std::cmp::min(from_block + batch - 1, safe);
        self.address_cache.refresh_if_stale(current).await?;

        let mut all = Vec::new();
        for h in from_block..=to_block {
            match self.scan_block(h).await {
                Ok(deps) => {
                    for d in deps {
                        if self.db.find_deposit_by_tx_hash(&d.tx_hash).await?.is_none() {
                            self.db.insert_deposit(&d).await?;
                            all.push(d);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(height = h, error = %e, "Tron scan failed");
                    return Ok((all, h));
                }
            }
        }

        tracing::info!(from = from_block, to = to_block, deposits = all.len(), "Tron scan");
        Ok((all, to_block + 1))
    }

    async fn current_block_height(&self) -> Result<u64> {
        let resp: serde_json::Value = self.client
            .get(format!("{}/wallet/getnowblock", self.api_url))
            .send().await.context("Tron getnowblock")?
            .json().await.context("Tron nowblock parse")?;

        resp["block_header"]["raw_data"]["number"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("No block number in response"))
    }
}

/// Convert sun to TRX. 1 TRX = 1,000,000 sun.
pub fn sun_to_trx(sun: u64) -> Decimal {
    Decimal::new(sun as i64, 6)
}

/// Convert TRX to sun.
pub fn trx_to_sun(trx: Decimal) -> u64 {
    use rust_decimal::prelude::ToPrimitive;
    (trx * Decimal::new(1_000_000, 0)).trunc().to_u64().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn sun_trx_conversion() {
        assert_eq!(sun_to_trx(1_000_000), dec!(1.000000));
        assert_eq!(sun_to_trx(500_000), dec!(0.500000));
        assert_eq!(trx_to_sun(dec!(1.0)), 1_000_000);
    }

    #[test]
    fn trc20_usdt_contract() {
        assert!(TRC20_USDT_CONTRACT.starts_with('T'));
        assert_eq!(TRC20_USDT_CONTRACT.len(), 34);
    }

    #[test]
    fn transfer_topic_is_keccak_of_transfer_event() {
        // keccak256("Transfer(address,address,uint256)") = ddf252ad...
        assert_eq!(TRANSFER_TOPIC.len(), 64);
        assert!(TRANSFER_TOPIC.starts_with("ddf252ad"));
    }

    #[test]
    fn hex_to_base58_valid() {
        // Known: 41a614f803b6fd780986a42c78ec9c7f77e6ded13c = TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t (USDT)
        let result = TronScanner::hex_to_base58("41a614f803b6fd780986a42c78ec9c7f77e6ded13c");
        assert!(result.is_some());
        let addr = result.unwrap();
        assert!(addr.starts_with('T'));
        assert_eq!(addr.len(), 34);
    }

    #[test]
    fn hex_to_base58_invalid() {
        // Too short
        assert!(TronScanner::hex_to_base58("41abcd").is_none());
        // Wrong prefix
        assert!(TronScanner::hex_to_base58("00a614f803b6fd780986a42c78ec9c7f77e6ded13c").is_none());
    }

    #[test]
    fn trc20_amount_parsing() {
        // 1,000,000 = 1 USDT (6 decimals)
        let amount_hex = "00000000000000000000000000000000000000000000000000000000000f4240";
        let amount_raw = u128::from_str_radix(amount_hex.trim_start_matches("0x"), 16).unwrap();
        assert_eq!(amount_raw, 1_000_000);
        let decimal = Decimal::new(amount_raw as i64, 6);
        assert_eq!(decimal, dec!(1.000000));
    }

    #[test]
    fn trc20_large_amount_parsing() {
        // 100,000 USDT = 100_000_000_000 base units
        let amount_raw: u128 = 100_000_000_000;
        let decimal = Decimal::new(amount_raw as i64, 6);
        assert_eq!(decimal, dec!(100000.000000));
    }

    #[test]
    fn trc20_token_config() {
        // Verify the default token config is correct
        let token = Trc20Token {
            symbol: "USDT".into(),
            contract_address: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".into(),
            contract_hex: "41a614f803b6fd780986a42c78ec9c7f77e6ded13c".into(),
            decimals: 6,
        };
        assert_eq!(token.symbol, "USDT");
        assert_eq!(token.decimals, 6);
        assert!(token.contract_hex.starts_with("41"));
        // Verify Base58Check matches hex
        let base58 = TronScanner::hex_to_base58(&token.contract_hex).unwrap();
        assert_eq!(base58, token.contract_address);
    }
}
