//! Backup verification — automated testing of backup/recovery procedures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Backup verification test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVerification {
    pub id: Uuid,
    pub backup_type: BackupType,
    pub test_result: VerificationResult,
    pub seed_hash_match: bool,
    pub shamir_reconstruct_ok: bool,
    pub db_restore_ok: bool,
    pub config_restore_ok: bool,
    pub duration_ms: u64,
    pub verified_at: DateTime<Utc>,
    pub next_verification_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupType { FullSystem, SeedOnly, DatabaseOnly, ConfigOnly, ShamirShares }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult { Pass, Fail, PartialPass, Skipped }

/// Backup schedule configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub full_backup_interval_hours: u32,
    pub incremental_interval_hours: u32,
    pub verification_interval_hours: u32,
    pub retention_days: u32,
    pub destinations: Vec<BackupDestination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDestination {
    pub name: String,
    pub dest_type: DestinationType,
    pub path: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationType { LocalDisk, S3, Gcs, Azure, Sftp }

impl Default for BackupSchedule {
    fn default() -> Self {
        Self {
            full_backup_interval_hours: 24,
            incremental_interval_hours: 1,
            verification_interval_hours: 168, // weekly
            retention_days: 90,
            destinations: vec![],
        }
    }
}

/// Database point-in-time recovery info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitrInfo {
    pub oldest_available: DateTime<Utc>,
    pub latest_available: DateTime<Utc>,
    pub wal_size_mb: u64,
    pub base_backup_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_verification_pass() {
        let v = BackupVerification {
            id: Uuid::nil(), backup_type: BackupType::FullSystem,
            test_result: VerificationResult::Pass,
            seed_hash_match: true, shamir_reconstruct_ok: true,
            db_restore_ok: true, config_restore_ok: true,
            duration_ms: 5000, verified_at: Utc::now(),
            next_verification_at: None,
        };
        assert_eq!(v.test_result, VerificationResult::Pass);
        assert!(v.seed_hash_match);
    }

    #[test]
    fn backup_schedule_default() {
        let s = BackupSchedule::default();
        assert_eq!(s.full_backup_interval_hours, 24);
        assert_eq!(s.retention_days, 90);
    }

    #[test]
    fn backup_type_serialization() {
        assert_eq!(serde_json::to_string(&BackupType::ShamirShares).unwrap(), r#""shamir_shares""#);
    }

    #[test]
    fn destination_types() {
        assert_eq!(serde_json::to_string(&DestinationType::S3).unwrap(), r#""s3""#);
        assert_eq!(serde_json::to_string(&DestinationType::Gcs).unwrap(), r#""gcs""#);
    }
}
