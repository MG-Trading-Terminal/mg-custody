//! Tron deposit scanner.
//!
//! Scans Tron blocks via HTTP API for TRX native transfers and
//! TRC-20 Transfer events to our deposit addresses.
//!
//! Tron specifics:
//! - HTTP API (not JSON-RPC): api.trongrid.io
//! - TRC-20 USDT: TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t
//! - 20 confirmations required for deposits
//! - Base58Check addresses (0x41 prefix)

use anyhow::Result;
use custody_common::{Chain, Deposit};
use custody_db::CustodyDb;

use crate::address_cache::AddressCache;
use crate::ChainScanner;

/// Tron block scanner using Tron HTTP API.
pub struct TronScanner {
    api_url: String,
    client: reqwest::Client,
    db: CustodyDb,
    address_cache: AddressCache,
    required_confirmations: u64,
}

impl TronScanner {
    pub fn new(
        api_url: &str,
        db: CustodyDb,
        address_cache: AddressCache,
        required_confirmations: u64,
    ) -> Self {
        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
            db,
            address_cache,
            required_confirmations,
        }
    }
}

impl ChainScanner for TronScanner {
    fn chain(&self) -> Chain {
        Chain::Tron
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        // TODO: Implement using Tron HTTP API
        // 1. GET /wallet/getnowblock → current block
        // 2. For each block from from_block to (current - 20):
        //    a. POST /wallet/getblockbynum {num: N}
        //    b. Parse transactions for TRX transfers and TRC-20 events
        //    c. Match against our address cache
        tracing::info!(from_block, "Tron scan (stub)");
        Ok((vec![], from_block))
    }

    async fn current_block_height(&self) -> Result<u64> {
        // TODO: GET /wallet/getnowblock → block_header.raw_data.number
        Ok(0)
    }
}

/// TRC-20 USDT mainnet contract address.
pub const TRC20_USDT_CONTRACT: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

/// Convert sun (Tron's smallest unit) to TRX.
/// 1 TRX = 1,000,000 sun
pub fn sun_to_trx(sun: u64) -> rust_decimal::Decimal {
    rust_decimal::Decimal::new(sun as i64, 6)
}

/// Convert TRX to sun.
pub fn trx_to_sun(trx: rust_decimal::Decimal) -> u64 {
    use rust_decimal::prelude::ToPrimitive;
    let sun = trx * rust_decimal::Decimal::new(1_000_000, 0);
    sun.trunc().to_u64().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn sun_trx_conversion() {
        assert_eq!(sun_to_trx(1_000_000), dec!(1.000000));
        assert_eq!(sun_to_trx(500_000), dec!(0.500000));
        assert_eq!(sun_to_trx(1), dec!(0.000001));

        assert_eq!(trx_to_sun(dec!(1.0)), 1_000_000);
        assert_eq!(trx_to_sun(dec!(0.5)), 500_000);
    }

    #[test]
    fn trc20_usdt_contract() {
        assert!(TRC20_USDT_CONTRACT.starts_with('T'));
        assert_eq!(TRC20_USDT_CONTRACT.len(), 34);
    }
}
