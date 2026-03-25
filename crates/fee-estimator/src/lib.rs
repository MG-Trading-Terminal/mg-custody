//! Network fee estimation per chain.
//!
//! Provides a `FeeEstimator` trait and per-chain implementations.
//! - Ethereum: EIP-1559 baseFee + priorityFee via alloy
//! - Bitcoin: `estimatesmartfee` (Phase 3)
//! - Tron: Energy/bandwidth costs (Phase 3)
//! - Solana: `getFeeForMessage` (Phase 3)

pub mod bitcoin;
pub mod ethereum;
pub mod solana;
pub mod tron;

use chrono::{DateTime, Utc};
use custody_common::Chain;
use rust_decimal::Decimal;

/// A fee estimate for a chain at a point in time.
#[derive(Debug, Clone)]
pub struct FeeEstimate {
    pub chain: Chain,
    /// Base fee in gwei (Ethereum) or sat/vB (Bitcoin).
    pub base_fee: Decimal,
    /// Priority fee / tip in gwei.
    pub priority_fee: Decimal,
    /// Recommended max fee: `2 * base_fee + priority_fee` (EIP-1559 heuristic).
    pub max_fee: Decimal,
    /// When this estimate was computed.
    pub estimated_at: DateTime<Utc>,
}

impl FeeEstimate {
    /// Compute the standard EIP-1559 max fee recommendation.
    ///
    /// `max_fee = 2 * base_fee + priority_fee`
    ///
    /// This ensures the transaction remains valid for ~2 consecutive full blocks
    /// of base fee increases (each block can increase base fee by 12.5%).
    pub fn eip1559(base_fee: Decimal, priority_fee: Decimal) -> Self {
        let max_fee = base_fee * Decimal::new(2, 0) + priority_fee;
        Self {
            chain: Chain::Ethereum,
            base_fee,
            priority_fee,
            max_fee,
            estimated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn eip1559_fee_calculation() {
        let estimate = FeeEstimate::eip1559(dec!(30), dec!(2));
        // max_fee = 2 * 30 + 2 = 62
        assert_eq!(estimate.max_fee, dec!(62));
        assert_eq!(estimate.base_fee, dec!(30));
        assert_eq!(estimate.priority_fee, dec!(2));
        assert_eq!(estimate.chain, Chain::Ethereum);
    }

    #[test]
    fn eip1559_fee_with_decimals() {
        let estimate = FeeEstimate::eip1559(dec!(25.5), dec!(1.5));
        // max_fee = 2 * 25.5 + 1.5 = 52.5
        assert_eq!(estimate.max_fee, dec!(52.5));
    }

    #[test]
    fn eip1559_zero_priority_fee() {
        let estimate = FeeEstimate::eip1559(dec!(100), dec!(0));
        assert_eq!(estimate.max_fee, dec!(200));
    }
}
