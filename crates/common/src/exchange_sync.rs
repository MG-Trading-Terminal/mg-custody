//! Exchange balance sync — real-time balance reconciliation with exchanges.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::exchange_api::Exchange;

/// Exchange balance snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeSnapshot {
    pub id: Uuid,
    pub exchange: Exchange,
    pub account_id: String,
    pub balances: Vec<ExchangeAssetBalance>,
    pub total_usd: Option<Decimal>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeAssetBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
    pub total: Decimal,
    pub usd_value: Option<Decimal>,
}

/// Discrepancy between expected and actual exchange balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceDiscrepancy {
    pub exchange: Exchange,
    pub asset: String,
    pub expected: Decimal,
    pub actual: Decimal,
    pub difference: Decimal,
    pub severity: DiscrepancySeverity,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscrepancySeverity { Info, Warning, Critical }

/// Calculate discrepancy severity.
pub fn classify_discrepancy(expected: Decimal, actual: Decimal) -> DiscrepancySeverity {
    if expected == Decimal::ZERO { return DiscrepancySeverity::Info; }
    let diff = (expected - actual).abs();
    let pct = diff / expected * Decimal::new(100, 0);
    if pct > Decimal::new(1, 0) { DiscrepancySeverity::Critical }
    else if pct > Decimal::new(1, 2) { DiscrepancySeverity::Warning } // 0.01%
    else { DiscrepancySeverity::Info }
}

/// Sync schedule per exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSchedule {
    pub exchange: Exchange,
    pub interval_seconds: u32,
    pub last_sync: Option<DateTime<Utc>>,
    pub next_sync: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}

/// Default sync intervals.
pub fn default_sync_intervals() -> Vec<(Exchange, u32)> {
    vec![
        (Exchange::Binance, 30),
        (Exchange::Coinbase, 60),
        (Exchange::Kraken, 60),
        (Exchange::Okx, 30),
        (Exchange::Bybit, 30),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn discrepancy_info() {
        assert_eq!(classify_discrepancy(dec!(1000), dec!(999.999)), DiscrepancySeverity::Info);
    }

    #[test]
    fn discrepancy_warning() {
        assert_eq!(classify_discrepancy(dec!(1000), dec!(999.5)), DiscrepancySeverity::Warning);
    }

    #[test]
    fn discrepancy_critical() {
        assert_eq!(classify_discrepancy(dec!(1000), dec!(980)), DiscrepancySeverity::Critical);
    }

    #[test]
    fn discrepancy_zero_expected() {
        assert_eq!(classify_discrepancy(Decimal::ZERO, dec!(100)), DiscrepancySeverity::Info);
    }

    #[test]
    fn default_sync_intervals_count() {
        assert!(default_sync_intervals().len() >= 5);
    }

    #[test]
    fn snapshot_structure() {
        let snap = ExchangeSnapshot {
            id: Uuid::nil(), exchange: Exchange::Binance,
            account_id: "main".into(),
            balances: vec![ExchangeAssetBalance {
                asset: "BTC".into(), free: dec!(1.5), locked: dec!(0.5),
                total: dec!(2.0), usd_value: Some(dec!(120000)),
            }],
            total_usd: Some(dec!(120000)), synced_at: Utc::now(),
        };
        assert_eq!(snap.balances.len(), 1);
    }

    #[test]
    fn severity_serialization() {
        assert_eq!(serde_json::to_string(&DiscrepancySeverity::Critical).unwrap(), r#""critical""#);
    }
}
