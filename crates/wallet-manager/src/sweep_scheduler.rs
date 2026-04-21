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
            if let Err(e) = self.tick().await {
                tracing::error!(error = %e, "Sweep scheduler tick failed");
            }
            tokio::time::sleep(self.hot_interval).await;
        }
    }

    async fn tick(&self) -> Result<()> {
        // Check all assets across all chains
        let assets = [
            ("ETH", Chain::Ethereum),
            ("BTC", Chain::Bitcoin),
            ("SOL", Chain::Solana),
            ("TRX", Chain::Tron),
            ("USDT", Chain::Ethereum),
            ("USDC", Chain::Ethereum),
        ];

        let mut all_recommendations = Vec::new();
        for (asset, chain) in &assets {
            // Canonical ledger key casing is the asset symbol
            // verbatim ("hot:ETH"), not lowercased. The prior
            // `asset.to_lowercase()` produced `"hot:eth"` which
            // matched NO ledger rows — every balance came back
            // zero, the `> 0` gate never fired, and the entire
            // tiered-storage sweep (hot/warm/cold 5/15/80) was
            // effectively dead code. Hot wallet could accumulate
            // 100% of funds and the scheduler would never notice.
            // record_sweep below already uses the uppercase
            // convention via `{asset}` interpolation, so keeping
            // it consistent here closes the loop.
            let hot = self.db.account_balance(&format!("hot:{asset}"), asset).await.unwrap_or(Decimal::ZERO);
            let warm = self.db.account_balance(&format!("warm:{asset}"), asset).await.unwrap_or(Decimal::ZERO);
            let cold = self.db.account_balance(&format!("cold:{asset}"), asset).await.unwrap_or(Decimal::ZERO);

            if hot + warm + cold > Decimal::ZERO {
                let recs = self.check_and_recommend(asset, *chain, hot, warm, cold);
                all_recommendations.extend(recs);
            }
        }

        let recommendations = all_recommendations;

        for rec in &recommendations {
            tracing::info!(
                asset = %rec.asset,
                direction = ?rec.direction,
                amount = %rec.amount,
                priority = ?rec.priority,
                "Sweep recommended"
            );

            // Record sweep in the double-entry ledger
            let (from_tier, to_tier) = match rec.direction {
                SweepDirection::WarmToHot => ("warm", "hot"),
                SweepDirection::HotToWarm => ("hot", "warm"),
                SweepDirection::WarmToCold => ("warm", "cold"),
                SweepDirection::ColdToWarm => ("cold", "warm"),
            };
            let sweep_id = uuid::Uuid::new_v4();
            if let Err(e) = self.db.record_sweep(
                sweep_id, from_tier, to_tier, &rec.asset, rec.chain, rec.amount,
            ).await {
                tracing::error!(error = %e, "Failed to record sweep");
            }
        }

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
