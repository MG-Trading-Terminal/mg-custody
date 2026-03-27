//! Risk engine — real-time transaction risk scoring, velocity checks, anomaly detection.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::Chain;

/// Real-time risk assessment for a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub transaction_id: Uuid,
    pub overall_score: f32,
    pub risk_level: RiskLevel,
    pub checks: Vec<RiskCheck>,
    pub decision: RiskDecision,
    pub assessed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel { Low, Medium, High, Critical }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskDecision { Allow, RequireApproval, Delay, Block }

/// Individual risk check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheck {
    pub check_type: RiskCheckType,
    pub passed: bool,
    pub score: f32,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskCheckType {
    SanctionsScreening,
    VelocityLimit,
    AmountThreshold,
    NewAddress,
    UnusualTime,
    CountryRestriction,
    DeviceFingerprint,
    AddressPoisoning,
    ContractInteraction,
    MevExposure,
}

/// Velocity check — track outflow over rolling windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityWindow {
    pub chain: Chain,
    pub window_minutes: u32,
    pub current_volume_usd: Decimal,
    pub limit_usd: Decimal,
    pub tx_count: u32,
    pub max_tx_count: u32,
}

impl VelocityWindow {
    pub fn is_exceeded(&self) -> bool {
        self.current_volume_usd > self.limit_usd || self.tx_count > self.max_tx_count
    }

    pub fn utilization_percent(&self) -> f32 {
        if self.limit_usd == Decimal::ZERO { return 0.0; }
        let ratio = self.current_volume_usd / self.limit_usd;
        ratio.to_string().parse::<f32>().unwrap_or(0.0) * 100.0
    }
}

/// Score a transaction based on multiple risk factors.
pub fn score_transaction(
    amount_usd: Decimal,
    is_new_address: bool,
    sanctions_clear: bool,
    velocity_ok: bool,
    is_contract_call: bool,
) -> (f32, RiskDecision) {
    let mut score: f32 = 0.0;

    if !sanctions_clear { return (1.0, RiskDecision::Block); }

    if amount_usd > Decimal::new(100_000, 0) { score += 0.3; }
    else if amount_usd > Decimal::new(10_000, 0) { score += 0.1; }

    if is_new_address { score += 0.2; }
    if !velocity_ok { score += 0.3; }
    if is_contract_call { score += 0.1; }

    let decision = if score >= 0.7 { RiskDecision::Block }
        else if score >= 0.5 { RiskDecision::RequireApproval }
        else if score >= 0.3 { RiskDecision::Delay }
        else { RiskDecision::Allow };

    (score.min(1.0), decision)
}

/// Anomaly detection — flag unusual patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyAlert {
    pub alert_type: AnomalyType,
    pub severity: RiskLevel,
    pub description: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    UnusualVolume,
    NewDestinationBurst,
    OffHoursActivity,
    RapidFireTransactions,
    LargeRoundAmount,
    CircuitBreakerNear,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn score_clean_transaction() {
        let (score, decision) = score_transaction(dec!(100), false, true, true, false);
        assert!(score < 0.3);
        assert_eq!(decision, RiskDecision::Allow);
    }

    #[test]
    fn score_sanctions_block() {
        let (score, decision) = score_transaction(dec!(1), false, false, true, false);
        assert_eq!(score, 1.0);
        assert_eq!(decision, RiskDecision::Block);
    }

    #[test]
    fn score_high_risk() {
        let (score, decision) = score_transaction(dec!(200000), true, true, false, true);
        assert!(score >= 0.7);
        assert_eq!(decision, RiskDecision::Block);
    }

    #[test]
    fn score_medium_risk() {
        let (score, decision) = score_transaction(dec!(50000), true, true, true, false);
        assert!(score >= 0.3);
        assert!(matches!(decision, RiskDecision::Delay | RiskDecision::RequireApproval));
    }

    #[test]
    fn velocity_not_exceeded() {
        let v = VelocityWindow {
            chain: Chain::Ethereum, window_minutes: 60,
            current_volume_usd: dec!(5000000), limit_usd: dec!(10000000),
            tx_count: 50, max_tx_count: 100,
        };
        assert!(!v.is_exceeded());
        assert!(v.utilization_percent() < 60.0);
    }

    #[test]
    fn velocity_exceeded() {
        let v = VelocityWindow {
            chain: Chain::Ethereum, window_minutes: 60,
            current_volume_usd: dec!(11000000), limit_usd: dec!(10000000),
            tx_count: 50, max_tx_count: 100,
        };
        assert!(v.is_exceeded());
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn risk_decision_serialization() {
        assert_eq!(serde_json::to_string(&RiskDecision::RequireApproval).unwrap(), r#""require_approval""#);
    }

    #[test]
    fn anomaly_types() {
        assert_eq!(serde_json::to_string(&AnomalyType::RapidFireTransactions).unwrap(), r#""rapid_fire_transactions""#);
    }

    #[test]
    fn risk_check_types() {
        let types = vec![
            RiskCheckType::SanctionsScreening, RiskCheckType::VelocityLimit,
            RiskCheckType::AddressPoisoning, RiskCheckType::MevExposure,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: RiskCheckType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, parsed);
        }
    }
}
