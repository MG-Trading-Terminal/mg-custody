//! Ethereum EIP-1559 fee estimation via alloy.
//!
//! Reads the latest block's `baseFeePerGas` and the node's `eth_maxPriorityFeePerGas`
//! to produce a fee recommendation.
//!
//! Reference: <https://eips.ethereum.org/EIPS/eip-1559>

use alloy::providers::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use rust_decimal::Decimal;

use crate::FeeEstimate;

/// Ethereum fee estimator using an alloy JSON-RPC provider.
pub struct EthereumFeeEstimator {
    provider: alloy::providers::fillers::FillProvider<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller,
                    >,
                >,
            >,
        >,
        alloy::providers::RootProvider,
    >,
    /// Maximum gas price (gwei) from config. Withdrawals are queued when exceeded.
    max_gas_price_gwei: u64,
}

impl EthereumFeeEstimator {
    /// Create a new Ethereum fee estimator.
    pub fn new(rpc_url: &str, max_gas_price_gwei: u64) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect_http(rpc_url.parse().context("Invalid RPC URL")?);
        Ok(Self {
            provider,
            max_gas_price_gwei,
        })
    }

    /// Estimate current gas fees.
    ///
    /// Returns base_fee and priority_fee in gwei, plus the recommended max_fee.
    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // Get latest block for baseFeePerGas
        let block = self
            .provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .context("Failed to get latest block")?
            .context("Latest block not found")?;

        let base_fee_wei = block.header.base_fee_per_gas.unwrap_or(0);

        // Get priority fee from node
        let priority_fee_wei: u128 = self
            .provider
            .get_max_priority_fee_per_gas()
            .await
            .context("Failed to get max priority fee")?;

        // Convert from wei to gwei (divide by 1e9)
        let base_fee_gwei = wei_to_gwei(base_fee_wei as u128);
        let priority_fee_gwei = wei_to_gwei(priority_fee_wei);

        Ok(FeeEstimate::eip1559(base_fee_gwei, priority_fee_gwei))
    }

    /// Check if current gas price exceeds the configured cap.
    ///
    /// When true, withdrawals should be queued rather than signed.
    pub async fn gas_exceeds_cap(&self) -> Result<bool> {
        let estimate = self.estimate().await?;
        let cap = Decimal::new(self.max_gas_price_gwei as i64, 0);
        Ok(estimate.base_fee > cap)
    }

    /// Get the configured gas cap in gwei.
    pub fn gas_cap_gwei(&self) -> u64 {
        self.max_gas_price_gwei
    }
}

/// Convert wei to gwei (divide by 10^9).
pub fn wei_to_gwei(wei: u128) -> Decimal {
    Decimal::new(wei as i64, 9)
}

/// Convert gwei to wei (multiply by 10^9).
pub fn gwei_to_wei(gwei: Decimal) -> u128 {
    let wei = gwei * Decimal::new(1_000_000_000, 0);
    use rust_decimal::prelude::ToPrimitive;
    wei.trunc().to_u128().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn wei_to_gwei_conversion() {
        // 30 gwei = 30_000_000_000 wei
        assert_eq!(wei_to_gwei(30_000_000_000), dec!(30.000000000));
        // 1 gwei = 1_000_000_000 wei
        assert_eq!(wei_to_gwei(1_000_000_000), dec!(1.000000000));
        // 0 wei = 0 gwei
        assert_eq!(wei_to_gwei(0), dec!(0.000000000));
        // 1.5 gwei = 1_500_000_000 wei
        assert_eq!(wei_to_gwei(1_500_000_000), dec!(1.500000000));
    }

    #[test]
    fn gwei_to_wei_conversion() {
        assert_eq!(gwei_to_wei(dec!(30)), 30_000_000_000u128);
        assert_eq!(gwei_to_wei(dec!(1)), 1_000_000_000u128);
        assert_eq!(gwei_to_wei(dec!(0)), 0u128);
        assert_eq!(gwei_to_wei(dec!(1.5)), 1_500_000_000u128);
    }

    #[test]
    fn gas_cap_check_logic() {
        // If base_fee is 150 gwei and cap is 200 gwei → not exceeded
        let estimate = FeeEstimate::eip1559(dec!(150), dec!(2));
        let cap = dec!(200);
        assert!(estimate.base_fee <= cap);

        // If base_fee is 250 gwei and cap is 200 gwei → exceeded
        let estimate = FeeEstimate::eip1559(dec!(250), dec!(2));
        assert!(estimate.base_fee > cap);
    }
}
