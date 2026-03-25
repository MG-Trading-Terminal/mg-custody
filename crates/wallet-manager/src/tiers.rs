//! Wallet tier configuration and monitoring.
//!
//! Tracks balances across hot/warm/cold tiers and flags when
//! rebalancing is needed.

use custody_common::WalletTier;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Configuration for wallet tier thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Maximum percentage of AUM in hot wallet.
    pub hot_max_pct: Decimal,
    /// Maximum percentage of AUM in warm wallet.
    pub warm_max_pct: Decimal,
    /// Target percentage for hot (refill trigger).
    pub hot_target_pct: Decimal,
    /// Minimum hot balance before auto-refill triggers.
    pub hot_min_pct: Decimal,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            hot_max_pct: dec!(5),
            warm_max_pct: dec!(15),
            hot_target_pct: dec!(3),
            hot_min_pct: dec!(1),
        }
    }
}

/// Current status of wallet tiers for an asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierStatus {
    pub asset: String,
    pub hot_balance: Decimal,
    pub warm_balance: Decimal,
    pub cold_balance: Decimal,
    pub total_aum: Decimal,
    pub hot_pct: Decimal,
    pub warm_pct: Decimal,
    pub cold_pct: Decimal,
}

impl TierStatus {
    /// Calculate tier status from balances.
    pub fn from_balances(asset: &str, hot: Decimal, warm: Decimal, cold: Decimal) -> Self {
        let total = hot + warm + cold;
        let (hot_pct, warm_pct, cold_pct) = if total > Decimal::ZERO {
            (
                (hot / total) * dec!(100),
                (warm / total) * dec!(100),
                (cold / total) * dec!(100),
            )
        } else {
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
        };

        Self {
            asset: asset.to_string(),
            hot_balance: hot,
            warm_balance: warm,
            cold_balance: cold,
            total_aum: total,
            hot_pct,
            warm_pct,
            cold_pct,
        }
    }
}

/// Wallet tier manager.
pub struct WalletTierManager {
    config: TierConfig,
}

impl WalletTierManager {
    pub fn new(config: TierConfig) -> Self {
        Self { config }
    }

    /// Check if hot wallet needs refilling.
    pub fn hot_needs_refill(&self, status: &TierStatus) -> bool {
        status.hot_pct < self.config.hot_min_pct && status.warm_balance > Decimal::ZERO
    }

    /// Check if hot wallet exceeds maximum.
    pub fn hot_exceeds_max(&self, status: &TierStatus) -> bool {
        status.hot_pct > self.config.hot_max_pct
    }

    /// Calculate how much to sweep from warm to hot (refill).
    pub fn hot_refill_amount(&self, status: &TierStatus) -> Decimal {
        if !self.hot_needs_refill(status) {
            return Decimal::ZERO;
        }
        let target = status.total_aum * self.config.hot_target_pct / dec!(100);
        let needed = target - status.hot_balance;
        // Don't exceed available warm balance
        needed.min(status.warm_balance).max(Decimal::ZERO)
    }

    /// Calculate how much to sweep from hot to warm (excess).
    pub fn hot_excess_amount(&self, status: &TierStatus) -> Decimal {
        if !self.hot_exceeds_max(status) {
            return Decimal::ZERO;
        }
        let target = status.total_aum * self.config.hot_target_pct / dec!(100);
        (status.hot_balance - target).max(Decimal::ZERO)
    }

    /// Determine which tier a new withdrawal should come from.
    pub fn withdrawal_tier(&self, amount: Decimal, status: &TierStatus) -> WalletTier {
        if amount <= status.hot_balance {
            WalletTier::Hot
        } else if amount <= status.hot_balance + status.warm_balance {
            WalletTier::Warm
        } else {
            WalletTier::Cold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_manager() -> WalletTierManager {
        WalletTierManager::new(TierConfig::default())
    }

    #[test]
    fn tier_status_calculation() {
        let status = TierStatus::from_balances("ETH", dec!(5), dec!(15), dec!(80));
        assert_eq!(status.total_aum, dec!(100));
        assert_eq!(status.hot_pct, dec!(5));
        assert_eq!(status.warm_pct, dec!(15));
        assert_eq!(status.cold_pct, dec!(80));
    }

    #[test]
    fn tier_status_zero_total() {
        let status = TierStatus::from_balances("ETH", dec!(0), dec!(0), dec!(0));
        assert_eq!(status.hot_pct, dec!(0));
    }

    #[test]
    fn hot_needs_refill_when_below_min() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(0.5), dec!(15), dec!(84.5));
        // hot_pct = 0.5% < hot_min_pct (1%)
        assert!(mgr.hot_needs_refill(&status));
    }

    #[test]
    fn hot_does_not_need_refill_when_above_min() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(3), dec!(15), dec!(82));
        // hot_pct = 3% > hot_min_pct (1%)
        assert!(!mgr.hot_needs_refill(&status));
    }

    #[test]
    fn hot_exceeds_max() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(10), dec!(15), dec!(75));
        // hot_pct = 10% > hot_max_pct (5%)
        assert!(mgr.hot_exceeds_max(&status));
    }

    #[test]
    fn hot_refill_amount() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(0.5), dec!(15), dec!(84.5));
        // Need to refill to 3% of 100 = 3 ETH. Currently 0.5 → need 2.5
        let amount = mgr.hot_refill_amount(&status);
        assert_eq!(amount, dec!(2.5));
    }

    #[test]
    fn hot_excess_amount() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(10), dec!(15), dec!(75));
        // Target 3% of 100 = 3 ETH. Have 10 → excess 7
        let amount = mgr.hot_excess_amount(&status);
        assert_eq!(amount, dec!(7));
    }

    #[test]
    fn withdrawal_tier_selection() {
        let mgr = default_manager();
        let status = TierStatus::from_balances("ETH", dec!(5), dec!(15), dec!(80));

        assert_eq!(mgr.withdrawal_tier(dec!(3), &status), WalletTier::Hot);
        assert_eq!(mgr.withdrawal_tier(dec!(10), &status), WalletTier::Warm);
        assert_eq!(mgr.withdrawal_tier(dec!(50), &status), WalletTier::Cold);
    }
}
