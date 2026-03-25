//! Bitcoin fee estimation.
//!
//! Uses Bitcoin Core's `estimatesmartfee` RPC to get the recommended
//! fee rate in sat/vB for a target number of confirmation blocks.
//!
//! Reference: Bitcoin Core `estimatesmartfee` command.

use anyhow::Result;
use rust_decimal::Decimal;

use crate::FeeEstimate;
use custody_common::Chain;

/// Default confirmation target: 3-6 blocks (~30-60 minutes).
pub const DEFAULT_TARGET_BLOCKS: u32 = 6;

/// Minimum fee rate (1 sat/vB).
pub const MIN_FEE_RATE_SAT_PER_VB: u64 = 1;

/// Bitcoin fee estimator.
pub struct BitcoinFeeEstimator {
    rpc_url: String,
    target_blocks: u32,
}

impl BitcoinFeeEstimator {
    pub fn new(rpc_url: &str, target_blocks: u32) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            target_blocks,
        }
    }

    /// Estimate the fee rate from Bitcoin Core.
    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // TODO: Call estimatesmartfee RPC
        // Returns BTC/kB → convert to sat/vB
        // For now return a default estimate
        let sat_per_vb = Decimal::new(10, 0); // 10 sat/vB default
        Ok(FeeEstimate {
            chain: Chain::Bitcoin,
            base_fee: sat_per_vb,
            priority_fee: Decimal::ZERO, // Bitcoin doesn't have priority fee
            max_fee: sat_per_vb,
            estimated_at: chrono::Utc::now(),
        })
    }
}

/// Convert BTC/kB (as returned by estimatesmartfee) to sat/vB.
///
/// 1 BTC = 100,000,000 sats, 1 kB = 1000 vbytes
/// sat/vB = (BTC/kB * 100,000,000) / 1000 = BTC/kB * 100,000
pub fn btc_per_kb_to_sat_per_vb(btc_per_kb: f64) -> u64 {
    let sat_per_vb = btc_per_kb * 100_000.0;
    std::cmp::max(sat_per_vb.round() as u64, MIN_FEE_RATE_SAT_PER_VB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_rate_conversion() {
        // 0.0001 BTC/kB = 10 sat/vB
        assert_eq!(btc_per_kb_to_sat_per_vb(0.0001), 10);
        // 0.00001 BTC/kB = 1 sat/vB
        assert_eq!(btc_per_kb_to_sat_per_vb(0.00001), 1);
        // 0.001 BTC/kB = 100 sat/vB
        assert_eq!(btc_per_kb_to_sat_per_vb(0.001), 100);
        // Very low → minimum 1 sat/vB
        assert_eq!(btc_per_kb_to_sat_per_vb(0.0), 1);
    }
}
