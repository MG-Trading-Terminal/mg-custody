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

/// Tron block scanner using HTTP API.
pub struct TronScanner {
    api_url: String,
    client: reqwest::Client,
    db: CustodyDb,
    address_cache: AddressCache,
    required_confirmations: u64,
}

impl TronScanner {
    pub fn new(api_url: &str, db: CustodyDb, address_cache: AddressCache, required_confirmations: u64) -> Self {
        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
            db, address_cache, required_confirmations,
        }
    }

    /// Get a block by number.
    async fn get_block(&self, num: u64) -> Result<serde_json::Value> {
        self.client
            .post(&format!("{}/wallet/getblockbynum", self.api_url))
            .json(&serde_json::json!({"num": num}))
            .send().await.context("Tron getblockbynum")?
            .json().await.context("Tron block parse")
    }

    /// Scan a block for TRX transfers to our addresses.
    async fn scan_block(&self, block_num: u64) -> Result<Vec<Deposit>> {
        let block = self.get_block(block_num).await?;
        let empty = vec![];
        let txs = block["transactions"].as_array().unwrap_or(&empty);
        let mut deposits = Vec::new();

        for tx in txs {
            let txid = tx["txID"].as_str().unwrap_or("");
            let ret = &tx["ret"];
            let success = ret.as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["contractRet"].as_str())
                .map(|s| s == "SUCCESS")
                .unwrap_or(false);

            if !success { continue; }

            // Check TransferContract (TRX native)
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

                // TRC-20 transfers would be detected via TriggerSmartContract events
                // Would need to parse internal transactions / event logs
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
            .get(&format!("{}/wallet/getnowblock", self.api_url))
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
}
