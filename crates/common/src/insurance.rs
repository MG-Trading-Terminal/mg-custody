//! Insurance & risk management — coverage types, claims, risk limits.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Insurance policy covering custody assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsurancePolicy {
    pub id: Uuid,
    pub provider: String,
    pub policy_number: String,
    pub coverage_type: CoverageType,
    pub coverage_limit_usd: Decimal,
    pub deductible_usd: Decimal,
    pub premium_annual_usd: Decimal,
    pub covered_chains: Vec<String>,
    pub exclusions: Vec<String>,
    pub effective_from: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: PolicyStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageType { CrimeFidelity, CyberLiability, ProfessionalIndemnity, DirectCustody, ColdStorage, HotWallet }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus { Active, Pending, Expired, Cancelled, ClaimFiled }

/// Insurance claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsuranceClaim {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub incident_id: Uuid,
    pub amount_claimed_usd: Decimal,
    pub amount_approved_usd: Option<Decimal>,
    pub description: String,
    pub status: ClaimStatus,
    pub filed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus { Filed, UnderReview, Approved, Denied, Paid }

/// Incident response playbook step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    pub order: u32,
    pub action: String,
    pub responsible: String,
    pub timeout_minutes: u32,
    pub auto_trigger: bool,
}

/// Pre-defined incident response playbooks.
pub fn security_incident_playbook() -> Vec<PlaybookStep> {
    vec![
        PlaybookStep { order: 1, action: "Freeze all withdrawals (emergency halt)".into(), responsible: "on-call".into(), timeout_minutes: 5, auto_trigger: true },
        PlaybookStep { order: 2, action: "Assess scope: which vaults/chains affected".into(), responsible: "security-lead".into(), timeout_minutes: 15, auto_trigger: false },
        PlaybookStep { order: 3, action: "Rotate API keys and HMAC secrets".into(), responsible: "infra".into(), timeout_minutes: 30, auto_trigger: false },
        PlaybookStep { order: 4, action: "Review audit logs for unauthorized access".into(), responsible: "security-lead".into(), timeout_minutes: 60, auto_trigger: false },
        PlaybookStep { order: 5, action: "Notify affected users/counterparties".into(), responsible: "operations".into(), timeout_minutes: 120, auto_trigger: false },
        PlaybookStep { order: 6, action: "File insurance claim if applicable".into(), responsible: "legal".into(), timeout_minutes: 1440, auto_trigger: false },
        PlaybookStep { order: 7, action: "Post-mortem: root cause, timeline, remediation".into(), responsible: "engineering".into(), timeout_minutes: 4320, auto_trigger: false },
    ]
}

/// Regulatory compliance matrix — track which regulations apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRequirement {
    pub regulation: String,
    pub jurisdiction: String,
    pub applicable: bool,
    pub status: ComplianceStatus,
    pub controls: Vec<String>,
    pub last_assessment: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceStatus { Compliant, PartiallyCompliant, NonCompliant, NotAssessed }

pub fn default_compliance_matrix() -> Vec<ComplianceRequirement> {
    vec![
        ComplianceRequirement { regulation: "FATF Travel Rule (R.16)".into(), jurisdiction: "Global".into(), applicable: true, status: ComplianceStatus::PartiallyCompliant, controls: vec!["travel_rule_v2".into()], last_assessment: None },
        ComplianceRequirement { regulation: "EU MiCA".into(), jurisdiction: "EU".into(), applicable: true, status: ComplianceStatus::NotAssessed, controls: vec![], last_assessment: None },
        ComplianceRequirement { regulation: "US BSA/AML".into(), jurisdiction: "US".into(), applicable: true, status: ComplianceStatus::PartiallyCompliant, controls: vec!["ofac_screening".into(), "sar_filing".into()], last_assessment: None },
        ComplianceRequirement { regulation: "SOC 2 Type 2".into(), jurisdiction: "Global".into(), applicable: true, status: ComplianceStatus::NonCompliant, controls: vec![], last_assessment: None },
        ComplianceRequirement { regulation: "CCSS Level 3".into(), jurisdiction: "Global".into(), applicable: true, status: ComplianceStatus::NonCompliant, controls: vec![], last_assessment: None },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn coverage_types() {
        assert_eq!(serde_json::to_string(&CoverageType::ColdStorage).unwrap(), r#""cold_storage""#);
    }

    #[test]
    fn incident_playbook_steps() {
        let steps = security_incident_playbook();
        assert!(steps.len() >= 7);
        assert_eq!(steps[0].order, 1);
        assert!(steps[0].auto_trigger); // first step auto
    }

    #[test]
    fn compliance_matrix() {
        let matrix = default_compliance_matrix();
        assert!(matrix.len() >= 5);
        let fatf = matrix.iter().find(|r| r.regulation.contains("FATF")).unwrap();
        assert!(fatf.applicable);
    }

    #[test]
    fn insurance_policy() {
        let policy = InsurancePolicy {
            id: Uuid::nil(), provider: "Lloyd's".into(), policy_number: "POL-001".into(),
            coverage_type: CoverageType::DirectCustody,
            coverage_limit_usd: dec!(50000000), deductible_usd: dec!(100000),
            premium_annual_usd: dec!(500000),
            covered_chains: vec!["ethereum".into(), "bitcoin".into()],
            exclusions: vec!["social engineering".into()],
            effective_from: Utc::now(), expires_at: Utc::now(),
            status: PolicyStatus::Active,
        };
        assert_eq!(policy.status, PolicyStatus::Active);
    }

    #[test]
    fn claim_status() {
        assert_eq!(serde_json::to_string(&ClaimStatus::UnderReview).unwrap(), r#""under_review""#);
    }

    #[test]
    fn compliance_status() {
        assert_eq!(serde_json::to_string(&ComplianceStatus::PartiallyCompliant).unwrap(), r#""partially_compliant""#);
    }
}
