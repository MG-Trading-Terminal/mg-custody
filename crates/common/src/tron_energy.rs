//! Tron Energy/Bandwidth estimation and resource management.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Tron resource types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TronResource {
    Bandwidth,
    Energy,
}

/// Tron account resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TronAccountResources {
    pub bandwidth_used: u64,
    pub bandwidth_limit: u64,
    pub energy_used: u64,
    pub energy_limit: u64,
    pub trx_balance_sun: u64,
}

impl TronAccountResources {
    pub fn bandwidth_available(&self) -> u64 {
        self.bandwidth_limit.saturating_sub(self.bandwidth_used)
    }

    pub fn energy_available(&self) -> u64 {
        self.energy_limit.saturating_sub(self.energy_used)
    }
}

/// Energy costs for common TRC-20 operations.
pub const ENERGY_TRC20_TRANSFER: u64 = 29_000;
pub const ENERGY_TRC20_APPROVE: u64 = 15_000;
pub const ENERGY_TRX_TRANSFER: u64 = 0; // uses bandwidth only
pub const BANDWIDTH_TRX_TRANSFER: u64 = 265;

/// Sun per Energy unit (approximate, depends on network).
pub const SUN_PER_ENERGY: u64 = 420;
/// TRX per Sun.
pub const SUN_PER_TRX: u64 = 1_000_000;

/// Estimate TRX cost for a TRC-20 transfer.
pub fn estimate_trc20_transfer_cost(energy_price_sun: u64) -> TronCostEstimate {
    let energy_cost_sun = ENERGY_TRC20_TRANSFER * energy_price_sun;
    let bandwidth_cost_sun = BANDWIDTH_TRX_TRANSFER * 1000; // 1000 sun per bandwidth unit (if no free)

    TronCostEstimate {
        energy_needed: ENERGY_TRC20_TRANSFER,
        bandwidth_needed: BANDWIDTH_TRX_TRANSFER,
        energy_cost_trx: Decimal::new(energy_cost_sun as i64, 6),
        bandwidth_cost_trx: Decimal::new(bandwidth_cost_sun as i64, 6),
        total_cost_trx: Decimal::new((energy_cost_sun + bandwidth_cost_sun) as i64, 6),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TronCostEstimate {
    pub energy_needed: u64,
    pub bandwidth_needed: u64,
    pub energy_cost_trx: Decimal,
    pub bandwidth_cost_trx: Decimal,
    pub total_cost_trx: Decimal,
}

/// Check if an account has enough resources for a TRC-20 transfer.
pub fn can_afford_trc20_transfer(resources: &TronAccountResources) -> ResourceCheck {
    let has_energy = resources.energy_available() >= ENERGY_TRC20_TRANSFER;
    let has_bandwidth = resources.bandwidth_available() >= BANDWIDTH_TRX_TRANSFER;

    // If no energy, need TRX to pay for it
    let trx_needed_sun = if !has_energy {
        ENERGY_TRC20_TRANSFER * SUN_PER_ENERGY
    } else { 0 } + if !has_bandwidth {
        BANDWIDTH_TRX_TRANSFER * 1000
    } else { 0 };

    ResourceCheck {
        has_energy, has_bandwidth,
        trx_needed: Decimal::new(trx_needed_sun as i64, 6),
        can_proceed: has_energy && has_bandwidth || resources.trx_balance_sun >= trx_needed_sun,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCheck {
    pub has_energy: bool,
    pub has_bandwidth: bool,
    pub trx_needed: Decimal,
    pub can_proceed: bool,
}

/// Tron staking for Energy (freeze TRX).
pub fn trx_to_freeze_for_energy(energy_needed: u64) -> u64 {
    // Approximate: 1 TRX frozen ≈ 1.5 Energy per day (varies with network)
    // For instant energy: energy_needed / rate
    (energy_needed as f64 / 1.5).ceil() as u64 * SUN_PER_TRX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_availability() {
        let res = TronAccountResources {
            bandwidth_used: 100, bandwidth_limit: 1000,
            energy_used: 5000, energy_limit: 50000,
            trx_balance_sun: 100_000_000,
        };
        assert_eq!(res.bandwidth_available(), 900);
        assert_eq!(res.energy_available(), 45000);
    }

    #[test]
    fn trc20_cost_estimate() {
        let est = estimate_trc20_transfer_cost(SUN_PER_ENERGY);
        assert_eq!(est.energy_needed, ENERGY_TRC20_TRANSFER);
        assert!(est.total_cost_trx > Decimal::ZERO);
    }

    #[test]
    fn can_afford_with_resources() {
        let res = TronAccountResources {
            bandwidth_used: 0, bandwidth_limit: 1000,
            energy_used: 0, energy_limit: 50000,
            trx_balance_sun: 0,
        };
        let check = can_afford_trc20_transfer(&res);
        assert!(check.has_energy);
        assert!(check.has_bandwidth);
        assert!(check.can_proceed);
    }

    #[test]
    fn can_afford_with_trx() {
        let res = TronAccountResources {
            bandwidth_used: 0, bandwidth_limit: 0,
            energy_used: 0, energy_limit: 0,
            trx_balance_sun: 100_000_000, // 100 TRX
        };
        let check = can_afford_trc20_transfer(&res);
        assert!(!check.has_energy);
        assert!(!check.has_bandwidth);
        assert!(check.can_proceed); // has TRX to pay
    }

    #[test]
    fn cannot_afford() {
        let res = TronAccountResources {
            bandwidth_used: 0, bandwidth_limit: 0,
            energy_used: 0, energy_limit: 0,
            trx_balance_sun: 0,
        };
        let check = can_afford_trc20_transfer(&res);
        assert!(!check.can_proceed);
    }

    #[test]
    fn freeze_for_energy() {
        let sun = trx_to_freeze_for_energy(29_000);
        assert!(sun > 0);
        // ~19334 TRX frozen = 29000 energy
        assert!(sun >= 19_000 * SUN_PER_TRX);
    }

    #[test]
    fn resource_type_serialization() {
        assert_eq!(serde_json::to_string(&TronResource::Energy).unwrap(), r#""energy""#);
        assert_eq!(serde_json::to_string(&TronResource::Bandwidth).unwrap(), r#""bandwidth""#);
    }
}
