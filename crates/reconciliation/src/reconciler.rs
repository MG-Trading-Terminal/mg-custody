//! On-chain vs internal ledger reconciliation.

use chrono::{DateTime, Utc};
use custody_common::Chain;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Discrepancy threshold (0.001%).
pub fn discrepancy_threshold_pct() -> Decimal {
    Decimal::new(1, 3) // 0.001
}

/// Result of reconciling one asset on one chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationResult {
    pub id: Uuid,
    pub chain: Chain,
    pub asset: String,
    pub on_chain_balance: Decimal,
    pub internal_balance: Decimal,
    pub discrepancy: Decimal,
    pub discrepancy_pct: Decimal,
    pub is_within_threshold: bool,
    pub reconciled_at: DateTime<Utc>,
}

impl ReconciliationResult {
    /// Reconcile an on-chain balance against internal ledger.
    pub fn reconcile(chain: Chain, asset: &str, on_chain: Decimal, internal: Decimal) -> Self {
        let discrepancy = (on_chain - internal).abs();
        let discrepancy_pct = if internal > Decimal::ZERO {
            (discrepancy / internal) * Decimal::new(100, 0)
        } else if on_chain > Decimal::ZERO {
            Decimal::new(100, 0) // 100% discrepancy if internal is 0 but on-chain isn't
        } else {
            Decimal::ZERO
        };
        let is_within_threshold = discrepancy_pct <= Decimal::new(1, 3); // 0.001%

        Self {
            id: Uuid::new_v4(),
            chain,
            asset: asset.to_string(),
            on_chain_balance: on_chain,
            internal_balance: internal,
            discrepancy,
            discrepancy_pct,
            is_within_threshold,
            reconciled_at: Utc::now(),
        }
    }
}

/// Alert for a discrepancy that exceeds the threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyAlert {
    pub reconciliation_id: Uuid,
    pub chain: Chain,
    pub asset: String,
    pub discrepancy: Decimal,
    pub discrepancy_pct: Decimal,
    pub severity: AlertSeverity,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Warning,  // > 0.001% but < 1%
    Critical, // > 1%
}

impl DiscrepancyAlert {
    pub fn from_result(result: &ReconciliationResult) -> Option<Self> {
        if result.is_within_threshold {
            return None;
        }
        let severity = if result.discrepancy_pct > Decimal::ONE {
            AlertSeverity::Critical
        } else {
            AlertSeverity::Warning
        };
        Some(Self {
            reconciliation_id: result.id,
            chain: result.chain,
            asset: result.asset.clone(),
            discrepancy: result.discrepancy,
            discrepancy_pct: result.discrepancy_pct,
            severity,
            created_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn balanced_reconciliation() {
        let result = ReconciliationResult::reconcile(Chain::Ethereum, "ETH", dec!(100.0), dec!(100.0));
        assert_eq!(result.discrepancy, dec!(0));
        assert!(result.is_within_threshold);
        assert!(DiscrepancyAlert::from_result(&result).is_none());
    }

    #[test]
    fn small_discrepancy_within_threshold() {
        // 0.0001% discrepancy
        let result = ReconciliationResult::reconcile(Chain::Ethereum, "ETH", dec!(100.0001), dec!(100.0));
        assert!(result.is_within_threshold);
    }

    #[test]
    fn large_discrepancy_triggers_alert() {
        // 5% discrepancy
        let result = ReconciliationResult::reconcile(Chain::Ethereum, "ETH", dec!(105), dec!(100));
        assert!(!result.is_within_threshold);
        let alert = DiscrepancyAlert::from_result(&result).unwrap();
        assert_eq!(alert.severity, AlertSeverity::Critical);
    }

    #[test]
    fn zero_internal_nonzero_onchain() {
        let result = ReconciliationResult::reconcile(Chain::Bitcoin, "BTC", dec!(1.0), dec!(0));
        assert!(!result.is_within_threshold);
        assert_eq!(result.discrepancy_pct, dec!(100));
    }

    #[test]
    fn both_zero_is_balanced() {
        let result = ReconciliationResult::reconcile(Chain::Solana, "SOL", dec!(0), dec!(0));
        assert!(result.is_within_threshold);
    }
}
