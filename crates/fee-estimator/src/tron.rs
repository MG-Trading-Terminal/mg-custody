//! Tron fee estimation (Energy/bandwidth model).
//!
//! TRC-20 transfers require Energy. Energy can be obtained by
//! freezing TRX or burning TRX directly.

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use crate::FeeEstimate;
use custody_common::Chain;

pub const TRC20_TRANSFER_ENERGY: u64 = 65_000;
pub const TRX_TRANSFER_BANDWIDTH: u64 = 270;
pub const ENERGY_PRICE_SUN: u64 = 420; // ~420 sun per Energy unit

pub struct TronFeeEstimator {
    api_url: String,
    client: reqwest::Client,
}

impl TronFeeEstimator {
    pub fn new(api_url: &str) -> Self {
        Self { api_url: api_url.to_string(), client: reqwest::Client::new() }
    }

    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // Query chain parameters for current energy price
        let resp: serde_json::Value = self.client
            .get(&format!("{}/wallet/getchainparameters", self.api_url))
            .send().await.context("Tron getchainparameters")?
            .json().await?;

        let energy_price = resp["chainParameter"].as_array()
            .and_then(|params| params.iter().find(|p| p["key"] == "getEnergyFee"))
            .and_then(|p| p["value"].as_u64())
            .unwrap_or(ENERGY_PRICE_SUN);

        let cost_sun = TRC20_TRANSFER_ENERGY * energy_price;
        let fee_trx = Decimal::new(cost_sun as i64, 6);

        Ok(FeeEstimate {
            chain: Chain::Tron,
            base_fee: fee_trx,
            priority_fee: Decimal::ZERO,
            max_fee: fee_trx,
            estimated_at: chrono::Utc::now(),
        })
    }

    /// Check account's available Energy.
    pub async fn get_energy(&self, address: &str) -> Result<u64> {
        let resp: serde_json::Value = self.client
            .post(&format!("{}/wallet/getaccountresource", self.api_url))
            .json(&serde_json::json!({"address": address, "visible": true}))
            .send().await?
            .json().await?;

        let energy_limit = resp["EnergyLimit"].as_u64().unwrap_or(0);
        let energy_used = resp["EnergyUsed"].as_u64().unwrap_or(0);
        Ok(energy_limit.saturating_sub(energy_used))
    }
}

pub fn trc20_burn_cost_sun(energy: u64, price: u64) -> u64 { energy * price }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_cost() {
        assert_eq!(trc20_burn_cost_sun(TRC20_TRANSFER_ENERGY, ENERGY_PRICE_SUN), 27_300_000);
    }
}
