//! Fee market analytics — mempool monitoring, fee prediction, congestion detection.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::Chain;

/// Fee market state for a chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeMarketState {
    pub chain: Chain,
    pub congestion: CongestionLevel,
    pub current_fee: Decimal,
    pub predicted_fee_1h: Decimal,
    pub predicted_fee_24h: Decimal,
    pub mempool_size: u64,
    pub mempool_bytes: u64,
    pub block_utilization: f32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CongestionLevel { Low, Normal, High, Critical }

/// Fee recommendation based on urgency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRecommendation {
    pub chain: Chain,
    pub economy: FeeOption,
    pub standard: FeeOption,
    pub fast: FeeOption,
    pub instant: FeeOption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeOption {
    pub label: String,
    pub fee_rate: Decimal,
    pub estimated_seconds: u32,
    pub confidence: f32,
}

/// Should we queue the transaction or send now?
pub fn should_queue(_chain: Chain, current_fee: Decimal, max_acceptable_fee: Decimal) -> QueueDecision {
    if current_fee <= max_acceptable_fee {
        QueueDecision::SendNow
    } else if current_fee <= max_acceptable_fee * Decimal::new(2, 0) {
        QueueDecision::WaitForDip { estimated_wait_minutes: 30 }
    } else {
        QueueDecision::QueueUntilBelow { target_fee: max_acceptable_fee }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum QueueDecision {
    SendNow,
    WaitForDip { estimated_wait_minutes: u32 },
    QueueUntilBelow { target_fee: Decimal },
}

/// Historical fee percentiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeePercentiles {
    pub chain: Chain,
    pub period: String,
    pub p10: Decimal,
    pub p25: Decimal,
    pub p50: Decimal,
    pub p75: Decimal,
    pub p90: Decimal,
    pub p99: Decimal,
}

/// Time-of-day fee pattern (useful for scheduling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeePattern {
    pub chain: Chain,
    pub cheapest_hour_utc: u8,
    pub most_expensive_hour_utc: u8,
    pub weekend_discount_percent: u32,
}

pub fn typical_fee_patterns() -> Vec<FeePattern> {
    vec![
        FeePattern { chain: Chain::Ethereum, cheapest_hour_utc: 4, most_expensive_hour_utc: 15, weekend_discount_percent: 30 },
        FeePattern { chain: Chain::Bitcoin, cheapest_hour_utc: 6, most_expensive_hour_utc: 18, weekend_discount_percent: 20 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn congestion_ordering() {
        assert!(CongestionLevel::Low < CongestionLevel::Normal);
        assert!(CongestionLevel::High < CongestionLevel::Critical);
    }

    #[test]
    fn should_queue_send_now() {
        let decision = should_queue(Chain::Ethereum, dec!(20), dec!(30));
        assert!(matches!(decision, QueueDecision::SendNow));
    }

    #[test]
    fn should_queue_wait() {
        let decision = should_queue(Chain::Ethereum, dec!(50), dec!(30));
        assert!(matches!(decision, QueueDecision::WaitForDip { .. }));
    }

    #[test]
    fn should_queue_until_below() {
        let decision = should_queue(Chain::Ethereum, dec!(100), dec!(30));
        assert!(matches!(decision, QueueDecision::QueueUntilBelow { .. }));
    }

    #[test]
    fn fee_patterns() {
        let patterns = typical_fee_patterns();
        assert!(patterns.len() >= 2);
        let eth = patterns.iter().find(|p| p.chain == Chain::Ethereum).unwrap();
        assert!(eth.cheapest_hour_utc < eth.most_expensive_hour_utc);
    }

    #[test]
    fn fee_market_state() {
        let state = FeeMarketState {
            chain: Chain::Ethereum, congestion: CongestionLevel::Normal,
            current_fee: dec!(25), predicted_fee_1h: dec!(22), predicted_fee_24h: dec!(18),
            mempool_size: 15000, mempool_bytes: 50_000_000,
            block_utilization: 0.85, updated_at: Utc::now(),
        };
        assert_eq!(state.congestion, CongestionLevel::Normal);
    }

    #[test]
    fn fee_recommendation() {
        let rec = FeeRecommendation {
            chain: Chain::Ethereum,
            economy: FeeOption { label: "Economy".into(), fee_rate: dec!(15), estimated_seconds: 600, confidence: 0.7 },
            standard: FeeOption { label: "Standard".into(), fee_rate: dec!(25), estimated_seconds: 120, confidence: 0.85 },
            fast: FeeOption { label: "Fast".into(), fee_rate: dec!(40), estimated_seconds: 30, confidence: 0.95 },
            instant: FeeOption { label: "Instant".into(), fee_rate: dec!(80), estimated_seconds: 12, confidence: 0.99 },
        };
        assert!(rec.economy.fee_rate < rec.fast.fee_rate);
    }

    #[test]
    fn congestion_serialization() {
        assert_eq!(serde_json::to_string(&CongestionLevel::Critical).unwrap(), r#""critical""#);
    }

    #[test]
    fn queue_decision_serialization() {
        let d = QueueDecision::WaitForDip { estimated_wait_minutes: 30 };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("wait_for_dip"));
    }
}
