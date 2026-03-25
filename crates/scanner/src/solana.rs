//! Solana deposit scanner.
//!
//! Detects SOL native transfers and SPL token transfers to our deposit addresses.
//!
//! Solana specifics:
//! - Uses `getSignaturesForAddress` per deposit address (not block scanning)
//! - "finalized" commitment level for crediting
//! - SPL token decimals: USDT=6, USDC=6
//! - Associated Token Accounts (ATAs) for SPL tokens

use anyhow::Result;
use custody_common::{Chain, Deposit};
use custody_db::CustodyDb;

use crate::ChainScanner;

/// Solana deposit scanner using Solana RPC.
pub struct SolanaScanner {
    rpc_url: String,
    db: CustodyDb,
    /// Known deposit addresses to monitor.
    deposit_addresses: Vec<String>,
    commitment: String,
}

impl SolanaScanner {
    pub fn new(rpc_url: &str, db: CustodyDb, commitment: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            db,
            deposit_addresses: Vec::new(),
            commitment: commitment.to_string(),
        }
    }
}

impl ChainScanner for SolanaScanner {
    fn chain(&self) -> Chain {
        Chain::Solana
    }

    async fn scan(&self, from_block: u64) -> Result<(Vec<Deposit>, u64)> {
        // TODO: Implement using solana-client
        // Solana scanning differs from block-based chains:
        // 1. For each deposit address, call getSignaturesForAddress
        // 2. For each new signature, getTransaction with maxSupportedTransactionVersion
        // 3. Parse pre/post balances to detect incoming transfers
        // 4. For SPL tokens, parse token balance changes
        tracing::info!(from_block, "Solana scan (stub)");
        Ok((vec![], from_block))
    }

    async fn current_block_height(&self) -> Result<u64> {
        // TODO: getSlot RPC with finalized commitment
        Ok(0)
    }
}

/// Lamports to SOL conversion.
/// 1 SOL = 1,000,000,000 lamports
pub fn lamports_to_sol(lamports: u64) -> rust_decimal::Decimal {
    rust_decimal::Decimal::new(lamports as i64, 9)
}

/// SOL to lamports conversion.
pub fn sol_to_lamports(sol: rust_decimal::Decimal) -> u64 {
    use rust_decimal::prelude::ToPrimitive;
    let lamports = sol * rust_decimal::Decimal::new(1_000_000_000, 0);
    lamports.trunc().to_u64().unwrap_or(0)
}

/// Blockhash TTL: Solana transactions expire after ~90 seconds.
pub const BLOCKHASH_TTL_SECS: u64 = 90;

/// Maximum retries with fresh blockhash before giving up.
pub const MAX_BLOCKHASH_RETRIES: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn lamports_sol_conversion() {
        assert_eq!(lamports_to_sol(1_000_000_000), dec!(1.000000000));
        assert_eq!(lamports_to_sol(500_000_000), dec!(0.500000000));
        assert_eq!(lamports_to_sol(1), dec!(0.000000001));
        assert_eq!(lamports_to_sol(0), dec!(0.000000000));

        assert_eq!(sol_to_lamports(dec!(1.0)), 1_000_000_000);
        assert_eq!(sol_to_lamports(dec!(0.5)), 500_000_000);
    }

    #[test]
    fn blockhash_ttl_reasonable() {
        assert!(BLOCKHASH_TTL_SECS >= 60);
        assert!(BLOCKHASH_TTL_SECS <= 150);
    }
}
