//! Gas Station — automatic gas funding for deposit addresses.
//!
//! When a deposit address needs to send ERC-20/TRC-20/SPL tokens,
//! it needs native gas (ETH/TRX/SOL). The Gas Station monitors balances
//! and auto-sends gas from a designated funding wallet.
//!
//! Modeled after Fireblocks Gas Station 2.0.

use custody_common::Chain;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Configuration for the Gas Station per chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasStationConfig {
    pub chain: Chain,
    /// Minimum gas balance — below this, auto-refuel triggers.
    pub gas_threshold: Decimal,
    /// Maximum gas balance to refuel to.
    pub gas_cap: Decimal,
    /// Maximum gas price to pay for the refueling tx (in native units).
    pub max_gas_price: Decimal,
    /// Whether the gas station is enabled for this chain.
    pub enabled: bool,
}

impl GasStationConfig {
    /// Default config for Ethereum (gas in ETH for ERC-20 operations).
    pub fn ethereum_default() -> Self {
        Self {
            chain: Chain::Ethereum,
            gas_threshold: dec!(0.01),   // 0.01 ETH
            gas_cap: dec!(0.05),          // 0.05 ETH
            max_gas_price: dec!(200),     // 200 gwei
            enabled: true,
        }
    }

    /// Default config for Tron (Energy for TRC-20 operations).
    pub fn tron_default() -> Self {
        Self {
            chain: Chain::Tron,
            gas_threshold: dec!(50),      // 50 TRX (for Energy)
            gas_cap: dec!(200),           // 200 TRX
            max_gas_price: dec!(1000),    // max sun per Energy unit
            enabled: true,
        }
    }

    /// Default config for Solana (SOL for SPL operations).
    pub fn solana_default() -> Self {
        Self {
            chain: Chain::Solana,
            gas_threshold: dec!(0.01),    // 0.01 SOL
            gas_cap: dec!(0.05),          // 0.05 SOL
            max_gas_price: dec!(0.001),   // max priority fee
            enabled: true,
        }
    }
}

/// Gas Station service.
pub struct GasStation {
    configs: Vec<GasStationConfig>,
}

/// A gas funding recommendation.
#[derive(Debug, Clone)]
pub struct GasFundingRequest {
    pub chain: Chain,
    pub address: String,
    pub current_balance: Decimal,
    pub fund_amount: Decimal,
}

/// Record of a gas funding transaction that was submitted to the withdrawal pipeline.
///
/// Each row represents one native-token transfer from the hot wallet to a deposit
/// address that needed gas for ERC-20 / TRC-20 / SPL token sweeps.
/// This is stored alongside the withdrawal record so operators can audit gas spend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GasFundingTx {
    /// Custody-internal ID, matches the `withdrawals.id` row inserted.
    pub withdrawal_id: uuid::Uuid,
    pub chain: Chain,
    /// Deposit address receiving the gas.
    pub target_address: String,
    /// Native gas token: ETH / TRX / SOL.
    pub gas_asset: String,
    /// Amount of native token sent.
    pub amount: Decimal,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl GasStation {
    pub fn new(configs: Vec<GasStationConfig>) -> Self {
        Self { configs }
    }

    /// Check if an address needs gas funding.
    pub fn needs_funding(&self, chain: Chain, current_balance: Decimal) -> Option<Decimal> {
        let config = self.configs.iter().find(|c| c.chain == chain && c.enabled)?;

        if current_balance < config.gas_threshold {
            let fund_amount = config.gas_cap - current_balance;
            Some(fund_amount.max(Decimal::ZERO))
        } else {
            None
        }
    }

    /// Get config for a chain.
    pub fn config_for_chain(&self, chain: Chain) -> Option<&GasStationConfig> {
        self.configs.iter().find(|c| c.chain == chain)
    }

    /// Process gas funding for a list of addresses.
    ///
    /// Returns a list of (address, chain, amount) tuples that need funding.
    /// The caller is responsible for creating the actual gas transfer transactions.
    pub fn check_funding_needs(
        &self,
        address_balances: &[(String, Chain, Decimal)],
    ) -> Vec<GasFundingRequest> {
        let mut requests = Vec::new();
        for (address, chain, balance) in address_balances {
            if let Some(amount) = self.needs_funding(*chain, *balance) {
                requests.push(GasFundingRequest {
                    chain: *chain,
                    address: address.clone(),
                    current_balance: *balance,
                    fund_amount: amount,
                });
            }
        }
        if !requests.is_empty() {
            tracing::info!(count = requests.len(), "Gas funding needed");
        }
        requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_gas_station() -> GasStation {
        GasStation::new(vec![
            GasStationConfig::ethereum_default(),
            GasStationConfig::tron_default(),
            GasStationConfig::solana_default(),
        ])
    }

    #[test]
    fn needs_funding_below_threshold() {
        let gs = default_gas_station();
        // ETH: 0.005 < threshold 0.01 → need funding
        let amount = gs.needs_funding(Chain::Ethereum, dec!(0.005));
        assert!(amount.is_some());
        // Should fund up to cap (0.05) - current (0.005) = 0.045
        assert_eq!(amount.unwrap(), dec!(0.045));
    }

    #[test]
    fn no_funding_above_threshold() {
        let gs = default_gas_station();
        // ETH: 0.03 > threshold 0.01 → no funding needed
        assert!(gs.needs_funding(Chain::Ethereum, dec!(0.03)).is_none());
    }

    #[test]
    fn funding_for_tron() {
        let gs = default_gas_station();
        // TRX: 10 < threshold 50 → need funding
        let amount = gs.needs_funding(Chain::Tron, dec!(10));
        assert!(amount.is_some());
        assert_eq!(amount.unwrap(), dec!(190)); // 200 - 10
    }

    #[test]
    fn no_funding_for_bitcoin() {
        let gs = default_gas_station();
        // Bitcoin has no gas station config
        assert!(gs.needs_funding(Chain::Bitcoin, dec!(0)).is_none());
    }

    #[test]
    fn disabled_chain_no_funding() {
        let mut config = GasStationConfig::ethereum_default();
        config.enabled = false;
        let gs = GasStation::new(vec![config]);
        assert!(gs.needs_funding(Chain::Ethereum, dec!(0.001)).is_none());
    }

    #[test]
    fn default_configs_reasonable() {
        let eth = GasStationConfig::ethereum_default();
        assert!(eth.gas_threshold < eth.gas_cap);
        assert!(eth.gas_threshold > Decimal::ZERO);

        let sol = GasStationConfig::solana_default();
        assert!(sol.gas_threshold < sol.gas_cap);
    }
}
