//! Reconciliation service — runs periodic checks comparing on-chain vs internal balances.

use std::time::Duration;

use custody_common::Chain;
use custody_db::CustodyDb;
use rust_decimal::Decimal;

use crate::eth_balance::EthBalanceProvider;
use crate::reconciler::{self, ReconciliationResult, DiscrepancyAlert};

/// Reconciliation service that periodically checks on-chain vs internal balances.
pub struct ReconciliationService {
    db: CustodyDb,
    eth_provider: Option<EthBalanceProvider>,
}

/// A balance snapshot for reconciliation (chain + asset + addresses with balances).
#[derive(Debug, Clone)]
pub struct BalanceSnapshot {
    pub chain: Chain,
    pub asset: String,
    pub on_chain_total: Decimal,
    pub internal_total: Decimal,
}

impl ReconciliationService {
    pub fn new(db: CustodyDb) -> Self {
        Self { db, eth_provider: None }
    }

    pub fn with_eth_rpc(mut self, rpc_url: &str) -> Self {
        self.eth_provider = Some(EthBalanceProvider::new(rpc_url));
        self
    }

    /// Reconcile a single asset by comparing on-chain balance against internal ledger.
    pub async fn reconcile_asset(
        &self,
        chain: Chain,
        asset: &str,
        on_chain_balance: Decimal,
    ) -> anyhow::Result<(ReconciliationResult, Option<DiscrepancyAlert>)> {
        // Get internal balance from ledger
        let hot = self.db.account_balance(&format!("hot:{asset}"), asset).await?;
        let warm = self.db.account_balance(&format!("warm:{asset}"), asset).await?;
        let cold = self.db.account_balance(&format!("cold:{asset}"), asset).await?;
        let internal_total = hot + warm + cold;

        let (result, alert) = reconciler::run_reconciliation(
            chain, asset, on_chain_balance, internal_total,
        );

        if let Some(ref alert) = alert {
            // Store alert for dashboard
            tracing::error!(
                chain = ?chain, asset,
                on_chain = %on_chain_balance,
                internal = %internal_total,
                discrepancy = %result.discrepancy,
                severity = ?alert.severity,
                "RECONCILIATION ALERT"
            );
        }

        Ok((result, alert))
    }

    /// Run reconciliation loop.
    pub async fn run(&self, interval: Duration) -> anyhow::Result<()> {
        tracing::info!("Reconciliation service started (interval: {:?})", interval);
        loop {
            if let Err(e) = self.tick().await {
                tracing::error!(error = %e, "Reconciliation tick failed");
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn tick(&self) -> anyhow::Result<()> {
        tracing::debug!("Reconciliation tick — checking balances");

        // 1. Ledger integrity check: total debits should equal total credits
        let assets = ["ETH", "BTC", "SOL", "TRX", "USDT", "USDC"];
        for asset in &assets {
            let total = self.db.total_asset_balance(asset).await?;
            if total != Decimal::ZERO {
                tracing::warn!(asset, total = %total, "Ledger imbalance detected");
            }
        }

        // 2. On-chain vs internal balance reconciliation (ETH)
        if let Some(ref eth) = self.eth_provider {
            self.reconcile_ethereum(eth).await?;
        }

        Ok(())
    }

    /// Walk every managed ETH address, sum on-chain balances, and
    /// compare to internal hot-wallet total.
    ///
    /// The old version used `.take(50)` to dodge RPC rate limits, but
    /// then compared the partial on-chain sum against the *full*
    /// internal total — guaranteeing a false-positive
    /// `DiscrepancyAlert` for any vault with more than 50 addresses
    /// (i.e., every serious one). Instead we paginate with a small
    /// inter-batch delay and bail out of the whole tick if too many
    /// RPCs fail, rather than comparing mismatched totals.
    async fn reconcile_ethereum(&self, eth: &EthBalanceProvider) -> anyhow::Result<()> {
        const BATCH_SIZE: usize = 25;
        const BATCH_DELAY: Duration = Duration::from_millis(200);
        /// Abort the whole pass if more than this fraction of RPC
        /// queries fail — a partial on-chain total vs full internal
        /// total is worse than no reconciliation at all.
        const MAX_FAILURE_RATIO: f64 = 0.10;

        let addresses = self.db.all_addresses_for_chain(Chain::Ethereum).await?;
        if addresses.is_empty() {
            tracing::debug!("No Ethereum addresses to reconcile");
            return Ok(());
        }

        let mut on_chain_total = Decimal::ZERO;
        let mut queried = 0usize;
        let mut failed = 0usize;

        for chunk in addresses.chunks(BATCH_SIZE) {
            for addr in chunk {
                match eth.native_balance(addr).await {
                    Ok(bal) => {
                        on_chain_total += bal;
                        queried += 1;
                    }
                    Err(e) => {
                        failed += 1;
                        tracing::debug!(addr, error = %e, "ETH balance query failed");
                    }
                }
            }
            tokio::time::sleep(BATCH_DELAY).await;
        }

        let total = queried + failed;
        let failure_ratio = failed as f64 / total.max(1) as f64;
        if failure_ratio > MAX_FAILURE_RATIO {
            tracing::warn!(
                queried,
                failed,
                ratio = %format_args!("{:.2}", failure_ratio),
                "Skipping ETH reconciliation — too many RPC failures to trust a partial total"
            );
            return Ok(());
        }

        // Account name is case-sensitive. Pipeline writes
        // ledger entries as `hot:{w.asset.as_str()}` where the
        // canonical asset code is uppercase ("ETH"), so the
        // account key is "hot:ETH". The prior hard-coded
        // "hot:eth" (lowercase) matched NO ledger entries — the
        // query returned 0 every tick, so the reconciler
        // compared the real on-chain total against zero and
        // fired a 100% discrepancy alert forever. Alert fatigue
        // then masked real divergence. Sum all three tiers to
        // match the reconcile_asset semantics elsewhere in this
        // file.
        let hot = self.db.account_balance("hot:ETH", "ETH").await?;
        let warm = self.db.account_balance("warm:ETH", "ETH").await?;
        let cold = self.db.account_balance("cold:ETH", "ETH").await?;
        let internal_total = hot + warm + cold;
        let (result, alert) = reconciler::run_reconciliation(
            Chain::Ethereum, "ETH", on_chain_total, internal_total,
        );

        if let Some(ref alert) = alert {
            tracing::error!(
                chain = "ethereum",
                addresses_total = addresses.len(),
                addresses_queried = queried,
                addresses_failed = failed,
                on_chain = %on_chain_total,
                internal = %internal_total,
                discrepancy = %result.discrepancy,
                severity = ?alert.severity,
                "RECONCILIATION ALERT — ETH balance mismatch"
            );
        } else {
            tracing::info!(
                addresses_queried = queried,
                addresses_failed = failed,
                on_chain = %on_chain_total,
                internal = %internal_total,
                "ETH reconciliation clean"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_snapshot_construction() {
        let snap = BalanceSnapshot {
            chain: Chain::Ethereum,
            asset: "ETH".into(),
            on_chain_total: Decimal::new(100, 0),
            internal_total: Decimal::new(100, 0),
        };
        assert_eq!(snap.on_chain_total, snap.internal_total);
    }
}
