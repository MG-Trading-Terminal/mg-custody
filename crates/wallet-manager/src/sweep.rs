//! Automated sweep recommendations.
//!
//! When tier thresholds are exceeded, generates sweep recommendations
//! that can be executed by the withdrawal pipeline.

use custody_common::{Chain, WalletTier};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Direction of a sweep transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepDirection {
    /// Hot → Warm (excess hot balance)
    HotToWarm,
    /// Warm → Hot (refill hot wallet)
    WarmToHot,
    /// Warm → Cold (excess warm balance)
    WarmToCold,
    /// Cold → Warm (refill warm wallet, requires ceremony)
    ColdToWarm,
}

/// A recommended sweep operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepRecommendation {
    pub id: Uuid,
    pub asset: String,
    pub chain: Chain,
    pub direction: SweepDirection,
    pub amount: Decimal,
    pub from_tier: WalletTier,
    pub to_tier: WalletTier,
    pub reason: String,
    pub priority: SweepPriority,
}

/// Priority of a sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepPriority {
    /// Can wait — optimization only.
    Low,
    /// Should happen soon — approaching threshold.
    Medium,
    /// Needs immediate action — hot wallet critically low or over max.
    High,
}

impl SweepRecommendation {
    /// Create a hot wallet refill recommendation.
    pub fn refill_hot(asset: &str, chain: Chain, amount: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset: asset.to_string(),
            chain,
            direction: SweepDirection::WarmToHot,
            amount,
            from_tier: WalletTier::Warm,
            to_tier: WalletTier::Hot,
            reason: "Hot wallet below minimum threshold".to_string(),
            priority: SweepPriority::High,
        }
    }

    /// Create a hot wallet excess sweep recommendation.
    pub fn sweep_hot_excess(asset: &str, chain: Chain, amount: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset: asset.to_string(),
            chain,
            direction: SweepDirection::HotToWarm,
            amount,
            from_tier: WalletTier::Hot,
            to_tier: WalletTier::Warm,
            reason: "Hot wallet exceeds maximum threshold".to_string(),
            priority: SweepPriority::Medium,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn sweep_recommendation_creation() {
        let rec = SweepRecommendation::refill_hot("ETH", Chain::Ethereum, dec!(2.5));
        assert_eq!(rec.direction, SweepDirection::WarmToHot);
        assert_eq!(rec.priority, SweepPriority::High);
        assert_eq!(rec.amount, dec!(2.5));
    }

    #[test]
    fn sweep_priority_ordering() {
        assert!(SweepPriority::Low < SweepPriority::Medium);
        assert!(SweepPriority::Medium < SweepPriority::High);
    }

    #[test]
    fn sweep_direction_serialization() {
        assert_eq!(
            serde_json::to_string(&SweepDirection::HotToWarm).unwrap(),
            "\"hot_to_warm\""
        );
    }
}
