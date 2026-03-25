//! Automated sweep scheduler.
//!
//! Runs as a background task. Checks tier balances periodically
//! and generates sweep transactions when thresholds are exceeded.

use std::time::Duration;
use anyhow::Result;
use custody_common::Chain;
use custody_db::CustodyDb;
use rust_decimal::Decimal;

use crate::tiers::{TierConfig, TierStatus, WalletTierManager};
use crate::sweep::{SweepRecommendation, SweepDirection};

/// Sweep scheduler configuration.
pub struct SweepScheduler {
    tier_manager: WalletTierManager,
    db: CustodyDb,
    /// Check interval for hot wallet.
    hot_interval: Duration,
    /// Check interval for warm/cold.
    cold_interval: Duration,
}

impl SweepScheduler {
    pub fn new(config: TierConfig, db: CustodyDb) -> Self {
        Self {
            tier_manager: WalletTierManager::new(config),
            db,
            hot_interval: Duration::from_secs(60),       // every minute
            cold_interval: Duration::from_secs(6 * 3600), // every 6 hours
        }
    }

    /// Check and generate sweep recommendations for an asset.
    pub fn check_and_recommend(
        &self,
        asset: &str,
        chain: Chain,
        hot: Decimal,
        warm: Decimal,
        cold: Decimal,
    ) -> Vec<SweepRecommendation> {
        let status = TierStatus::from_balances(asset, hot, warm, cold);
        let mut recommendations = Vec::new();

        if self.tier_manager.hot_needs_refill(&status) {
            let amount = self.tier_manager.hot_refill_amount(&status);
            if amount > Decimal::ZERO {
                recommendations.push(SweepRecommendation::refill_hot(asset, chain, amount));
            }
        }

        if self.tier_manager.hot_exceeds_max(&status) {
            let amount = self.tier_manager.hot_excess_amount(&status);
            if amount > Decimal::ZERO {
                recommendations.push(SweepRecommendation::sweep_hot_excess(asset, chain, amount));
            }
        }

        recommendations
    }

    /// Run the sweep scheduler loop.
    pub async fn run(&self) -> Result<()> {
        tracing::info!("Starting sweep scheduler");
        loop {
            // TODO: Query actual balances from DB/chain for each asset
            // For now, just sleep
            if let Err(e) = self.tick().await {
                tracing::error!(error = %e, "Sweep scheduler tick failed");
            }
            tokio::time::sleep(self.hot_interval).await;
        }
    }

    async fn tick(&self) -> Result<()> {
        // TODO: For each supported asset:
        // 1. Query hot/warm/cold balances
        // 2. Call check_and_recommend
        // 3. If recommendations, create sweep withdrawal records
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn check_balanced_no_recommendations() {
        // With balanced tiers, no sweeps needed
        let manager = WalletTierManager::new(TierConfig::default());
        let status = TierStatus::from_balances("ETH", dec!(3), dec!(15), dec!(82));
        assert!(!manager.hot_needs_refill(&status));
        assert!(!manager.hot_exceeds_max(&status));
    }
}
