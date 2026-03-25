//! Bitcoin deposit scanner.
//!
//! Scans Bitcoin blocks for incoming transactions to our P2WPKH deposit addresses.
//! Uses Bitcoin Core RPC (`getblock`, `getrawtransaction`).
//!
//! Bitcoin specifics:
//! - UTXO model: check each transaction output, not the `to` field
//! - P2WPKH outputs: match by scriptPubKey witness program
//! - 3 confirmations required for deposits

use anyhow::{Context, Result};
use chrono::Utc;
use custody_common::{Asset, Chain, Deposit, DepositStatus};
use custody_db::CustodyDb;
use uuid::Uuid;

use crate::address_cache::AddressCache;
use crate::ChainScanner;

/// Bitcoin block scanner using Bitcoin Core RPC.
pub struct BitcoinScanner {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
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
            db,
            address_cache,
            required_confirmations,
        }
    }
}

impl ChainScanner for BitcoinScanner {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        // TODO: Implement using bitcoincore-rpc
        // 1. Get current block count
        // 2. For each block from from_block to (current - required_confirmations):
        //    a. Get block hash via getblockhash
        //    b. Get block with transactions via getblock(hash, 2)
        //    c. For each transaction, check each output's scriptPubKey.address
        //    d. If address is in our cache, create a Deposit
        // 3. Dedup by txid:vout and persist
        tracing::info!(from_block, "Bitcoin scan (stub)");
        Ok((vec![], from_block))
    }

    async fn current_block_height(&self) -> Result<u64> {
        // TODO: getblockcount RPC
        Ok(0)
    }
}

/// Parse a Bitcoin amount from BTC to satoshis.
pub fn btc_to_sats(btc: f64) -> u64 {
    // Bitcoin Core returns amounts as f64 BTC
    // Convert carefully: multiply by 1e8 and round
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
}
