//! Compliance reporting — SAR filing, CTR reports, regulatory exports.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Suspicious Activity Report (SAR).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspiciousActivityReport {
    pub id: Uuid,
    pub reference_number: String,
    pub filing_type: SarFilingType,
    pub subject: SarSubject,
    pub transactions: Vec<Uuid>,
    pub total_amount_usd: Decimal,
    pub narrative: String,
    pub risk_indicators: Vec<String>,
    pub status: SarStatus,
    pub filed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SarFilingType { Initial, Continuing, Joint }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarSubject {
    pub name: Option<String>,
    pub address: String,
    pub chain: String,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SarStatus { Draft, UnderReview, Filed, Closed }

/// Currency Transaction Report (CTR) — required for transactions > $10K.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyTransactionReport {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub amount_usd: Decimal,
    pub transaction_type: String,
    pub conductor: CtrParty,
    pub status: CtrStatus,
    pub filing_deadline: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtrParty {
    pub name: String,
    pub identification_type: String,
    pub identification_number: String,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtrStatus { Pending, Filed, Late, Exempt }

/// Regulatory export — transaction data for auditors/regulators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegulatoryExport {
    pub id: Uuid,
    pub export_type: ExportType,
    pub date_from: DateTime<Utc>,
    pub date_to: DateTime<Utc>,
    pub format: ExportFormat,
    pub record_count: u64,
    pub file_hash: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportType { AllTransactions, Withdrawals, Deposits, SarFilings, Screenings }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat { Csv, Json, Xml, Pdf }

/// Key ceremony audit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyCeremonyAudit {
    pub id: Uuid,
    pub ceremony_type: CeremonyType,
    pub participants: Vec<CeremonyParticipant>,
    pub quorum_met: bool,
    pub video_recorded: bool,
    pub notarized: bool,
    pub status: CeremonyStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyType { KeyGeneration, KeyRotation, KeyRevocation, BackupRecovery, ShamirSplit }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyParticipant {
    pub name: String,
    pub role: String,
    pub present: bool,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyStatus { Scheduled, InProgress, Completed, Aborted }

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn sar_filing_types() {
        assert_eq!(serde_json::to_string(&SarFilingType::Continuing).unwrap(), r#""continuing""#);
    }

    #[test]
    fn ctr_above_10k() {
        let ctr = CurrencyTransactionReport {
            id: Uuid::nil(), transaction_id: Uuid::nil(),
            amount_usd: dec!(15000), transaction_type: "withdrawal".into(),
            conductor: CtrParty { name: "John Doe".into(), identification_type: "passport".into(), identification_number: "AB123456".into(), address: None },
            status: CtrStatus::Pending,
            filing_deadline: Utc::now(), created_at: Utc::now(),
        };
        assert!(ctr.amount_usd > dec!(10000));
    }

    #[test]
    fn export_formats() {
        assert_eq!(serde_json::to_string(&ExportFormat::Xml).unwrap(), r#""xml""#);
        assert_eq!(serde_json::to_string(&ExportFormat::Pdf).unwrap(), r#""pdf""#);
    }

    #[test]
    fn key_ceremony_audit() {
        let ceremony = KeyCeremonyAudit {
            id: Uuid::nil(), ceremony_type: CeremonyType::KeyGeneration,
            participants: vec![
                CeremonyParticipant { name: "Alice".into(), role: "operator".into(), present: true, signature: Some("sig1".into()) },
                CeremonyParticipant { name: "Bob".into(), role: "witness".into(), present: true, signature: Some("sig2".into()) },
            ],
            quorum_met: true, video_recorded: true, notarized: false,
            status: CeremonyStatus::Completed,
            started_at: Utc::now(), completed_at: Some(Utc::now()),
        };
        assert!(ceremony.quorum_met);
        assert_eq!(ceremony.participants.len(), 2);
    }

    #[test]
    fn ceremony_type_serialization() {
        assert_eq!(serde_json::to_string(&CeremonyType::ShamirSplit).unwrap(), r#""shamir_split""#);
    }

    #[test]
    fn regulatory_export() {
        let export = RegulatoryExport {
            id: Uuid::nil(), export_type: ExportType::SarFilings,
            date_from: Utc::now(), date_to: Utc::now(),
            format: ExportFormat::Csv, record_count: 42,
            file_hash: Some("sha256:abc".into()), generated_at: Utc::now(),
        };
        assert_eq!(export.record_count, 42);
    }
}
