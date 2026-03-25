//! Tron fee estimation (Energy/bandwidth model).
//!
//! Tron uses an Energy model for smart contract calls and bandwidth for
//! simple transfers. TRC-20 transfers require Energy, which can be obtained
//! by freezing TRX or paying in TRX (burning).
//!
//! Reference: <https://developers.tron.network/docs/resource-model>

use anyhow::Result;
use rust_decimal::Decimal;

use crate::FeeEstimate;
use custody_common::Chain;

/// Energy cost per TRC-20 transfer (approximate).
pub const TRC20_TRANSFER_ENERGY: u64 = 65_000;

/// Bandwidth cost per simple TRX transfer (bytes).
pub const TRX_TRANSFER_BANDWIDTH: u64 = 270;

/// Current Energy price in sun per unit (approximately 420 sun/Energy as of 2024).
pub const ENERGY_PRICE_SUN: u64 = 420;

/// Tron fee estimator.
pub struct TronFeeEstimator {
    api_url: String,
    client: reqwest::Client,
}

impl TronFeeEstimator {
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Estimate the fee for a TRC-20 transfer.
    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // TODO: Query account resources to check available Energy
        // For now, return the burn cost estimate
        let energy_cost_sun = TRC20_TRANSFER_ENERGY * ENERGY_PRICE_SUN;
        let fee_trx = Decimal::new(energy_cost_sun as i64, 6); // sun to TRX

        Ok(FeeEstimate {
            chain: Chain::Tron,
            base_fee: fee_trx,
            priority_fee: Decimal::ZERO,
            max_fee: fee_trx,
            estimated_at: chrono::Utc::now(),
        })
    }
}

/// Calculate the TRX burn cost for a TRC-20 transfer without Energy.
pub fn trc20_burn_cost_sun(energy_needed: u64, energy_price_sun: u64) -> u64 {
    energy_needed * energy_price_sun
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trc20_energy_cost() {
        let cost = trc20_burn_cost_sun(TRC20_TRANSFER_ENERGY, ENERGY_PRICE_SUN);
        // 65,000 * 420 = 27,300,000 sun = 27.3 TRX
        assert_eq!(cost, 27_300_000);
    }

    #[test]
    fn energy_constants_reasonable() {
        assert!(TRC20_TRANSFER_ENERGY > 10_000); // typical range
        assert!(TRC20_TRANSFER_ENERGY < 200_000);
        assert!(ENERGY_PRICE_SUN > 100);
        assert!(ENERGY_PRICE_SUN < 2000);
    }
}
