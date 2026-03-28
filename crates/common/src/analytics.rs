//! Internal analytics — transaction labeling, wallet scoring, volume tracking,
//! fee breakdown, and periodic summaries.

use chrono::{DateTime, NaiveDate, Utc};
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

// ---------------------------------------------------------------------------
// Transaction analytics + fee tracking (Feature 2)
// ---------------------------------------------------------------------------

/// Analytics period selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnalyticsPeriod {
    Day { date: NaiveDate },
    Week { start: NaiveDate },
    Month { year: i32, month: u32 },
    Custom { from: DateTime<Utc>, to: DateTime<Utc> },
}

impl AnalyticsPeriod {
    /// Inclusive start/end datetimes for this period.
    pub fn bounds(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        use chrono::{TimeZone, Duration};
        match self {
            AnalyticsPeriod::Day { date } => {
                let start = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
                let end   = start + Duration::days(1);
                (start, end)
            }
            AnalyticsPeriod::Week { start } => {
                let s = Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap());
                (s, s + Duration::weeks(1))
            }
            AnalyticsPeriod::Month { year, month } => {
                let start_date = NaiveDate::from_ymd_opt(*year, *month, 1).unwrap();
                let end_date = if *month == 12 {
                    NaiveDate::from_ymd_opt(*year + 1, 1, 1).unwrap()
                } else {
                    NaiveDate::from_ymd_opt(*year, *month + 1, 1).unwrap()
                };
                let s = Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap());
                let e = Utc.from_utc_datetime(&end_date.and_hms_opt(0, 0, 0).unwrap());
                (s, e)
            }
            AnalyticsPeriod::Custom { from, to } => (*from, *to),
        }
    }

    /// Human-readable label (e.g. "2026-03-27", "2026-W13", "2026-03").
    pub fn label(&self) -> String {
        match self {
            AnalyticsPeriod::Day { date } => date.format("%Y-%m-%d").to_string(),
            AnalyticsPeriod::Week { start } => {
                use chrono::Datelike;
                format!("{}-W{:02}", start.year(), start.iso_week().week())
            }
            AnalyticsPeriod::Month { year, month } => format!("{year}-{month:02}"),
            AnalyticsPeriod::Custom { from, to } => {
                format!("{}/{}", from.format("%Y-%m-%d"), to.format("%Y-%m-%d"))
            }
        }
    }
}

/// Aggregate transaction flow and fee metrics for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionAnalytics {
    pub period: AnalyticsPeriod,
    pub total_inflow_usd: Decimal,
    pub total_outflow_usd: Decimal,
    pub net_flow_usd: Decimal,
    pub total_fees_usd: Decimal,
    pub avg_fee_per_tx_usd: Decimal,
    pub tx_count: u64,
    pub deposits_count: u64,
    pub withdrawals_count: u64,
    /// Chain with the highest transaction count.
    pub busiest_chain: String,
    pub largest_tx_usd: Decimal,
}

impl TransactionAnalytics {
    /// Compute derived fields from raw inputs. All amounts must be Decimal.
    pub fn compute(
        period: AnalyticsPeriod,
        total_inflow_usd: Decimal,
        total_outflow_usd: Decimal,
        total_fees_usd: Decimal,
        tx_count: u64,
        deposits_count: u64,
        withdrawals_count: u64,
        busiest_chain: String,
        largest_tx_usd: Decimal,
    ) -> Self {
        let net_flow_usd = total_inflow_usd - total_outflow_usd;
        let avg_fee_per_tx_usd = if tx_count > 0 {
            total_fees_usd / Decimal::from(tx_count)
        } else {
            Decimal::ZERO
        };
        Self {
            period,
            total_inflow_usd,
            total_outflow_usd,
            net_flow_usd,
            total_fees_usd,
            avg_fee_per_tx_usd,
            tx_count,
            deposits_count,
            withdrawals_count,
            busiest_chain,
            largest_tx_usd,
        }
    }
}

/// Per-chain fee breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeBreakdown {
    pub chain: String,
    pub total_fees_native: Decimal,
    pub total_fees_usd: Decimal,
    pub avg_fee_native: Decimal,
    pub tx_count: u64,
}

impl FeeBreakdown {
    pub fn new(
        chain: String,
        total_fees_native: Decimal,
        total_fees_usd: Decimal,
        tx_count: u64,
    ) -> Self {
        let avg_fee_native = if tx_count > 0 {
            total_fees_native / Decimal::from(tx_count)
        } else {
            Decimal::ZERO
        };
        Self { chain, total_fees_native, total_fees_usd, avg_fee_native, tx_count }
    }
}

/// A time-series data point for flow charts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDataPoint {
    pub timestamp: DateTime<Utc>,
    pub inflow_usd: Decimal,
    pub outflow_usd: Decimal,
    pub fees_usd: Decimal,
    pub tx_count: u64,
}

/// Query parameters for the analytics summary endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSummaryQuery {
    pub period: String,   // "daily", "weekly", "monthly"
    pub date: Option<NaiveDate>,
    pub year: Option<i32>,
    pub month: Option<u32>,
}

impl AnalyticsSummaryQuery {
    /// Convert query params to an AnalyticsPeriod.
    pub fn to_period(&self) -> anyhow::Result<AnalyticsPeriod> {
        match self.period.as_str() {
            "daily" => {
                let date = self.date.ok_or_else(|| anyhow::anyhow!("daily period requires 'date' param"))?;
                Ok(AnalyticsPeriod::Day { date })
            }
            "weekly" => {
                let date = self.date.ok_or_else(|| anyhow::anyhow!("weekly period requires 'date' param (start of week)"))?;
                Ok(AnalyticsPeriod::Week { start: date })
            }
            "monthly" => {
                let year  = self.year.ok_or_else(|| anyhow::anyhow!("monthly period requires 'year'"))?;
                let month = self.month.ok_or_else(|| anyhow::anyhow!("monthly period requires 'month'"))?;
                if !(1..=12).contains(&month) {
                    anyhow::bail!("month must be 1-12, got {month}");
                }
                Ok(AnalyticsPeriod::Month { year, month })
            }
            other => anyhow::bail!("unknown period type: {other}"),
        }
    }
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

    // -- TransactionAnalytics --

    #[test]
    fn transaction_analytics_net_flow() {
        let a = TransactionAnalytics::compute(
            AnalyticsPeriod::Day { date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap() },
            dec!(500000),   // inflow
            dec!(300000),   // outflow
            dec!(1500),     // fees
            100,            // tx_count
            60,             // deposits
            40,             // withdrawals
            "ethereum".into(),
            dec!(50000),
        );
        assert_eq!(a.net_flow_usd, dec!(200000));
        assert_eq!(a.avg_fee_per_tx_usd, dec!(15));
        assert_eq!(a.deposits_count, 60);
        assert_eq!(a.withdrawals_count, 40);
    }

    #[test]
    fn transaction_analytics_zero_txs_no_divide_by_zero() {
        let a = TransactionAnalytics::compute(
            AnalyticsPeriod::Month { year: 2026, month: 3 },
            dec!(0), dec!(0), dec!(0), 0, 0, 0, "bitcoin".into(), dec!(0),
        );
        assert_eq!(a.avg_fee_per_tx_usd, dec!(0));
    }

    // -- FeeBreakdown --

    #[test]
    fn fee_breakdown_avg_calculation() {
        let fb = FeeBreakdown::new("ethereum".into(), dec!(0.015), dec!(45), 3);
        assert_eq!(fb.avg_fee_native, dec!(0.005));
        assert_eq!(fb.total_fees_usd, dec!(45));
    }

    #[test]
    fn fee_breakdown_zero_count() {
        let fb = FeeBreakdown::new("solana".into(), dec!(0), dec!(0), 0);
        assert_eq!(fb.avg_fee_native, dec!(0));
    }

    // -- AnalyticsPeriod --

    #[test]
    fn period_day_label() {
        let p = AnalyticsPeriod::Day { date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap() };
        assert_eq!(p.label(), "2026-03-27");
    }

    #[test]
    fn period_month_label() {
        let p = AnalyticsPeriod::Month { year: 2026, month: 3 };
        assert_eq!(p.label(), "2026-03");
    }

    #[test]
    fn period_month_bounds_march() {
        let p = AnalyticsPeriod::Month { year: 2026, month: 3 };
        let (start, end) = p.bounds();
        assert_eq!(start.format("%Y-%m-%d").to_string(), "2026-03-01");
        assert_eq!(end.format("%Y-%m-%d").to_string(), "2026-04-01");
    }

    #[test]
    fn period_month_bounds_december_wraps_year() {
        let p = AnalyticsPeriod::Month { year: 2026, month: 12 };
        let (_, end) = p.bounds();
        assert_eq!(end.format("%Y-%m-%d").to_string(), "2027-01-01");
    }

    #[test]
    fn analytics_query_daily_parse() {
        let q = AnalyticsSummaryQuery {
            period: "daily".into(),
            date: Some(NaiveDate::from_ymd_opt(2026, 3, 27).unwrap()),
            year: None, month: None,
        };
        let period = q.to_period().unwrap();
        assert_eq!(period, AnalyticsPeriod::Day { date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap() });
    }

    #[test]
    fn analytics_query_monthly_parse() {
        let q = AnalyticsSummaryQuery {
            period: "monthly".into(), date: None,
            year: Some(2026), month: Some(3),
        };
        let period = q.to_period().unwrap();
        assert_eq!(period, AnalyticsPeriod::Month { year: 2026, month: 3 });
    }

    #[test]
    fn analytics_query_invalid_month_rejected() {
        let q = AnalyticsSummaryQuery {
            period: "monthly".into(), date: None, year: Some(2026), month: Some(13),
        };
        assert!(q.to_period().is_err());
    }

    #[test]
    fn analytics_query_unknown_period_rejected() {
        let q = AnalyticsSummaryQuery {
            period: "quarterly".into(), date: None, year: None, month: None,
        };
        assert!(q.to_period().is_err());
    }
}
