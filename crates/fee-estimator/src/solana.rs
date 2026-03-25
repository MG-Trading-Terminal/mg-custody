//! Solana fee estimation.
//!
//! Uses `getFeeForMessage` RPC to estimate transaction fees.
//! Solana fees are generally very low (5000 lamports = 0.000005 SOL per signature).
//!
//! Reference: <https://docs.solana.com/developing/transaction_confirmation>

use anyhow::Result;
use rust_decimal::Decimal;

use crate::FeeEstimate;
use custody_common::Chain;

/// Base fee per signature in lamports.
pub const BASE_FEE_LAMPORTS: u64 = 5_000;

/// Solana fee estimator.
pub struct SolanaFeeEstimator {
    rpc_url: String,
}

impl SolanaFeeEstimator {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
        }
    }

    /// Estimate the fee for a Solana transaction.
    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // TODO: Call getFeeForMessage RPC for a more accurate estimate
        // For now, return the base fee (5000 lamports per signature)
        let fee_sol = Decimal::new(BASE_FEE_LAMPORTS as i64, 9); // lamports to SOL

        Ok(FeeEstimate {
            chain: Chain::Solana,
            base_fee: fee_sol,
            priority_fee: Decimal::ZERO, // priority fees are optional on Solana
            max_fee: fee_sol,
            estimated_at: chrono::Utc::now(),
        })
    }
}

/// Estimate the total fee for a transaction with N signatures.
pub fn estimate_fee_lamports(num_signatures: u32) -> u64 {
    BASE_FEE_LAMPORTS * num_signatures as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn base_fee_is_5000_lamports() {
        assert_eq!(BASE_FEE_LAMPORTS, 5_000);
        // In SOL: 0.000005
        assert_eq!(
            Decimal::new(BASE_FEE_LAMPORTS as i64, 9),
            dec!(0.000005000)
        );
    }

    #[test]
    fn multi_signature_fee() {
        assert_eq!(estimate_fee_lamports(1), 5_000);
        assert_eq!(estimate_fee_lamports(2), 10_000); // 2-sig tx
        assert_eq!(estimate_fee_lamports(3), 15_000); // multisig
    }
}
