//! Risk assessment aggregation.
//!
//! Combines results from multiple screening providers and sanctions lists
//! into a single risk decision for a transaction.

use chrono::{DateTime, Utc};
use custody_common::Chain;
use serde::{Deserialize, Serialize};

use crate::ofac::SanctionsCheckResult;
use crate::screening::{RiskLevel, ScreeningResult};

/// Aggregated risk assessment for a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub address: String,
    pub chain: Chain,
    /// Overall risk level (worst of all checks).
    pub overall_risk: RiskLevel,
    /// Whether the transaction should be blocked.
    pub blocked: bool,
    /// Whether manual review is needed.
    pub needs_review: bool,
    /// Reasons for the decision.
    pub reasons: Vec<String>,
    /// Individual screening results.
    pub screening_results: Vec<ScreeningResult>,
    /// Sanctions check result.
    pub sanctions_result: Option<SanctionsCheckResult>,
    pub assessed_at: DateTime<Utc>,
}

impl RiskAssessment {
    /// Create a risk assessment from screening and sanctions results.
    pub fn assess(
        address: &str,
        chain: Chain,
        screenings: Vec<ScreeningResult>,
        sanctions: Option<SanctionsCheckResult>,
    ) -> Self {
        let mut overall_risk = RiskLevel::Low;
        let mut blocked = false;
        let mut needs_review = false;
        let mut reasons = Vec::new();

        // Check sanctions first (highest priority)
        if let Some(ref s) = sanctions {
            if s.is_sanctioned {
                overall_risk = RiskLevel::Severe;
                blocked = true;
                reasons.push(format!(
                    "Sanctioned address (lists: {})",
                    s.matched_lists.join(", ")
                ));
            }
        }

        // Check screening results
        for result in &screenings {
            if result.risk_level > overall_risk {
                overall_risk = result.risk_level;
            }
            if result.should_block() {
                blocked = true;
                reasons.push(format!(
                    "{}: risk={:?}, categories={:?}",
                    result.provider, result.risk_level, result.categories
                ));
            }
            if result.needs_review() {
                needs_review = true;
            }
        }

        Self {
            address: address.to_string(),
            chain,
            overall_risk,
            blocked,
            needs_review,
            reasons,
            screening_results: screenings,
            sanctions_result: sanctions,
            assessed_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screening::ScreeningResult;

    fn clean_screening() -> ScreeningResult {
        ScreeningResult {
            address: "0xclean".to_string(),
            chain: Chain::Ethereum,
            risk_level: RiskLevel::Low,
            risk_score: 0.05,
            provider: "test".to_string(),
            categories: vec![],
            screened_at: Utc::now(),
            details: None,
        }
    }

    fn risky_screening() -> ScreeningResult {
        ScreeningResult {
            address: "0xrisky".to_string(),
            chain: Chain::Ethereum,
            risk_level: RiskLevel::High,
            risk_score: 0.85,
            provider: "test".to_string(),
            categories: vec!["mixer".to_string()],
            screened_at: Utc::now(),
            details: None,
        }
    }

    #[test]
    fn clean_assessment() {
        let assessment = RiskAssessment::assess(
            "0xclean",
            Chain::Ethereum,
            vec![clean_screening()],
            None,
        );
        assert_eq!(assessment.overall_risk, RiskLevel::Low);
        assert!(!assessment.blocked);
        assert!(!assessment.needs_review);
    }

    #[test]
    fn risky_assessment_blocks() {
        let assessment = RiskAssessment::assess(
            "0xrisky",
            Chain::Ethereum,
            vec![risky_screening()],
            None,
        );
        assert_eq!(assessment.overall_risk, RiskLevel::High);
        assert!(assessment.blocked);
    }

    #[test]
    fn sanctioned_always_blocks() {
        let sanctions = SanctionsCheckResult {
            address: "0xbad".to_string(),
            is_sanctioned: true,
            matched_lists: vec!["OFAC SDN".to_string()],
            checked_at: Utc::now(),
        };
        let assessment = RiskAssessment::assess(
            "0xbad",
            Chain::Ethereum,
            vec![clean_screening()], // even clean screening
            Some(sanctions),
        );
        assert_eq!(assessment.overall_risk, RiskLevel::Severe);
        assert!(assessment.blocked);
        assert!(assessment.reasons[0].contains("Sanctioned"));
    }

    #[test]
    fn worst_risk_wins() {
        let mut medium = clean_screening();
        medium.risk_level = RiskLevel::Medium;

        let assessment = RiskAssessment::assess(
            "0x",
            Chain::Ethereum,
            vec![clean_screening(), medium, risky_screening()],
            None,
        );
        assert_eq!(assessment.overall_risk, RiskLevel::High); // worst of Low, Medium, High
    }
}
