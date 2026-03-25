//! On-chain balance provider trait.
//!
//! Each chain implements this to query balances from the blockchain.

use anyhow::Result;
use custody_common::Chain;
use rust_decimal::Decimal;

/// Query on-chain balance for an address.
pub trait BalanceProvider: Send + Sync {
    fn chain(&self) -> Chain;

    /// Get the native token balance for an address.
    fn native_balance(&self, address: &str) -> impl std::future::Future<Output = Result<Decimal>> + Send;

    /// Get a token balance for an address.
    fn token_balance(&self, address: &str, contract: &str) -> impl std::future::Future<Output = Result<Decimal>> + Send;
}
