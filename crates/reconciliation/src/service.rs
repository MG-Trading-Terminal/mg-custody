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
            // Get all our ETH deposit addresses
            let addresses = self.db.all_addresses_for_chain(Chain::Ethereum).await?;

            // Sum on-chain balances for all managed addresses
            let mut on_chain_total = Decimal::ZERO;
            for addr in addresses.iter().take(50) { // limit to avoid rate limiting
                match eth.native_balance(addr).await {
                    Ok(bal) => on_chain_total += bal,
                    Err(e) => {
                        tracing::debug!(addr, error = %e, "Failed to query ETH balance");
                    }
                }
            }

            // Compare against internal hot wallet balance
            let internal_hot = self.db.account_balance("hot:eth", "ETH").await?;

            let (result, alert) = reconciler::run_reconciliation(
                Chain::Ethereum, "ETH", on_chain_total, internal_hot,
            );

            if let Some(ref alert) = alert {
                tracing::error!(
                    chain = "ethereum",
                    on_chain = %on_chain_total,
                    internal = %internal_hot,
                    discrepancy = %result.discrepancy,
                    severity = ?alert.severity,
                    "RECONCILIATION ALERT — ETH balance mismatch"
                );
            }
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
