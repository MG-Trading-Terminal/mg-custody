//! Internal analytics — transaction labeling, wallet scoring, volume tracking.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// Transaction label (user-assigned metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLabel {
    pub transaction_id: Uuid,
    pub labels: Vec<String>,
    pub department: Option<String>,
    pub cost_center: Option<String>,
    pub counterparty_name: Option<String>,
    pub purpose: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Volume statistics for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeStats {
    pub period: String,
    pub total_deposits: u64,
    pub total_withdrawals: u64,
    pub total_internal_transfers: u64,
    pub deposit_volume_usd: Decimal,
    pub withdrawal_volume_usd: Decimal,
    pub fees_paid_usd: Decimal,
    pub unique_addresses: u64,
    pub top_assets: Vec<AssetVolume>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetVolume {
    pub asset: String,
    pub chain: Chain,
    pub volume_usd: Decimal,
    pub transaction_count: u64,
}

/// Wallet risk score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletScore {
    pub address: String,
    pub chain: Chain,
    pub risk_score: f32,
    pub factors: Vec<RiskFactor>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    pub factor: String,
    pub weight: f32,
    pub value: String,
    pub impact: RiskImpact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskImpact { Positive, Neutral, Negative }

/// Dashboard KPIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardKpis {
    pub total_aum_usd: Decimal,
    pub daily_volume_usd: Decimal,
    pub active_vaults: u32,
    pub pending_withdrawals: u32,
    pub stuck_transactions: u32,
    pub circuit_breaker_utilization: f32,
    pub avg_withdrawal_time_secs: u32,
    pub uptime_percent: f32,
    pub timestamp: DateTime<Utc>,
}

/// Revenue tracking for the custody service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueMetrics {
    pub period: String,
    pub withdrawal_fees: Decimal,
    pub custody_fees: Decimal,
    pub staking_commissions: Decimal,
    pub total_revenue: Decimal,
    pub cost_of_gas: Decimal,
    pub net_revenue: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn transaction_label() {
        let label = TransactionLabel {
            transaction_id: Uuid::nil(), labels: vec!["payroll".into(), "q1".into()],
            department: Some("Engineering".into()), cost_center: Some("CC-100".into()),
            counterparty_name: Some("Acme Corp".into()), purpose: Some("Salary March".into()),
            updated_at: Utc::now(),
        };
        assert_eq!(label.labels.len(), 2);
    }

    #[test]
    fn volume_stats() {
        let stats = VolumeStats {
            period: "2026-03".into(), total_deposits: 1500,
            total_withdrawals: 800, total_internal_transfers: 200,
            deposit_volume_usd: dec!(50000000), withdrawal_volume_usd: dec!(30000000),
            fees_paid_usd: dec!(15000), unique_addresses: 450,
            top_assets: vec![AssetVolume { asset: "ETH".into(), chain: Chain::Ethereum, volume_usd: dec!(20000000), transaction_count: 500 }],
        };
        assert_eq!(stats.top_assets.len(), 1);
    }

    #[test]
    fn risk_impact_serialization() {
        assert_eq!(serde_json::to_string(&RiskImpact::Negative).unwrap(), r#""negative""#);
    }

    #[test]
    fn dashboard_kpis() {
        let kpis = DashboardKpis {
            total_aum_usd: dec!(100000000), daily_volume_usd: dec!(5000000),
            active_vaults: 50, pending_withdrawals: 12, stuck_transactions: 0,
            circuit_breaker_utilization: 0.15, avg_withdrawal_time_secs: 120,
            uptime_percent: 99.99, timestamp: Utc::now(),
        };
        assert_eq!(kpis.stuck_transactions, 0);
        assert!(kpis.uptime_percent > 99.0);
    }

    #[test]
    fn revenue_metrics() {
        let rev = RevenueMetrics {
            period: "2026-Q1".into(), withdrawal_fees: dec!(50000),
            custody_fees: dec!(100000), staking_commissions: dec!(20000),
            total_revenue: dec!(170000), cost_of_gas: dec!(15000),
            net_revenue: dec!(155000),
        };
        assert_eq!(rev.net_revenue, rev.total_revenue - rev.cost_of_gas);
    }
}
