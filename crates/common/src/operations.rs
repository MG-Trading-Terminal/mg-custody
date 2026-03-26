//! Operational types — scheduling, compliance screening, backup/recovery,
//! token discovery, health dashboard.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Asset, Chain};

// ---------------------------------------------------------------------------
// Transaction scheduling
// ---------------------------------------------------------------------------

/// A scheduled transaction for future execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTransaction {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub destination_address: String,
    pub amount: Decimal,
    pub scheduled_at: DateTime<Utc>,
    pub status: ScheduleStatus,
    /// Resulting withdrawal ID once executed.
    pub withdrawal_id: Option<Uuid>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub executed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Pending,
    Executed,
    Cancelled,
    Failed,
    Expired,
}

impl std::fmt::Display for ScheduleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Executed => f.write_str("executed"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed => f.write_str("failed"),
            Self::Expired => f.write_str("expired"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduledTxRequest {
    pub vault_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub destination_address: String,
    pub amount: Decimal,
    pub scheduled_at: DateTime<Utc>,
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Compliance screening results
// ---------------------------------------------------------------------------

/// Result of a compliance screening on an address or transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreeningResult {
    pub id: Uuid,
    pub address: String,
    pub chain: Chain,
    pub provider: String,
    pub risk_level: RiskLevel,
    pub risk_score: Option<f32>,
    pub flags: Vec<String>,
    pub sanctions_match: bool,
    pub screened_at: DateTime<Utc>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Severe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenAddressRequest {
    pub address: String,
    pub chain: Chain,
}

// ---------------------------------------------------------------------------
// Backup & recovery status
// ---------------------------------------------------------------------------

/// Key backup health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatus {
    pub seed_encrypted: bool,
    pub seed_backup_count: u32,
    pub shamir_threshold: u8,
    pub shamir_total_shares: u8,
    pub last_backup_at: Option<DateTime<Utc>>,
    pub last_verification_at: Option<DateTime<Utc>>,
    pub mpc_key_shares: Vec<MpcShareStatus>,
    pub backup_locations: Vec<BackupLocation>,
    pub recovery_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpcShareStatus {
    pub key_id: String,
    pub share_index: u16,
    pub stored_in: String,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupLocation {
    pub name: String,
    pub location_type: String,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Token discovery
// ---------------------------------------------------------------------------

/// Auto-detected token from a contract address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredToken {
    pub contract_address: String,
    pub chain: Chain,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub token_type: String,
    pub total_supply: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverTokenRequest {
    pub contract_address: String,
    pub chain: Chain,
}

// ---------------------------------------------------------------------------
// Health dashboard
// ---------------------------------------------------------------------------

/// Comprehensive system health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub version: String,
    pub database: ComponentHealth,
    pub chains: Vec<ChainHealthDetail>,
    pub pipeline: PipelineHealth,
    pub background_tasks: Vec<TaskHealth>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub latency_ms: u32,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainHealthDetail {
    pub chain: Chain,
    pub status: HealthStatus,
    pub block_height: u64,
    pub latency_ms: u32,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub pending_deposits: u32,
    pub pending_withdrawals: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineHealth {
    pub received: u32,
    pub validated: u32,
    pub signed: u32,
    pub broadcast: u32,
    pub confirming: u32,
    pub stuck_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHealth {
    pub name: String,
    pub running: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub interval_secs: u32,
    pub error_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn schedule_status_display() {
        assert_eq!(ScheduleStatus::Pending.to_string(), "pending");
        assert_eq!(ScheduleStatus::Executed.to_string(), "executed");
    }

    #[test]
    fn scheduled_tx_serialization() {
        let tx = ScheduledTransaction {
            id: Uuid::nil(), vault_id: Uuid::nil(),
            asset: Asset::new("ETH"), chain: Chain::Ethereum,
            destination_address: "0x123".into(), amount: dec!(1.5),
            scheduled_at: Utc::now(), status: ScheduleStatus::Pending,
            withdrawal_id: None, note: Some("Monthly salary".into()),
            created_at: Utc::now(), executed_at: None, cancelled_at: None,
        };
        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("Monthly salary"));
        assert!(json.contains("pending"));
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Severe);
    }

    #[test]
    fn screening_result_serialization() {
        let result = ScreeningResult {
            id: Uuid::nil(), address: "0xabc".into(),
            chain: Chain::Ethereum, provider: "chainalysis".into(),
            risk_level: RiskLevel::Low, risk_score: Some(0.15),
            flags: vec![], sanctions_match: false,
            screened_at: Utc::now(), details: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("chainalysis"));
        assert!(json.contains("0.15"));
    }

    #[test]
    fn backup_status_recovery_ready() {
        let status = BackupStatus {
            seed_encrypted: true, seed_backup_count: 3,
            shamir_threshold: 3, shamir_total_shares: 5,
            last_backup_at: Some(Utc::now()),
            last_verification_at: Some(Utc::now()),
            mpc_key_shares: vec![], backup_locations: vec![],
            recovery_ready: true,
        };
        assert!(status.recovery_ready);
        assert!(status.seed_backup_count >= status.shamir_threshold as u32);
    }

    #[test]
    fn discovered_token() {
        let token = DiscoveredToken {
            contract_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".into(),
            chain: Chain::Ethereum, symbol: "USDT".into(),
            name: "Tether USD".into(), decimals: 6,
            token_type: "ERC-20".into(),
            total_supply: Some("39000000000".into()),
            verified: true,
        };
        assert_eq!(token.decimals, 6);
        assert!(token.verified);
    }

    #[test]
    fn health_status_serialization() {
        assert_eq!(serde_json::to_string(&HealthStatus::Healthy).unwrap(), r#""healthy""#);
        assert_eq!(serde_json::to_string(&HealthStatus::Degraded).unwrap(), r#""degraded""#);
    }

    #[test]
    fn system_health_structure() {
        let health = SystemHealth {
            status: HealthStatus::Healthy,
            uptime_seconds: 86400,
            version: "0.1.0".into(),
            database: ComponentHealth { status: HealthStatus::Healthy, latency_ms: 5, details: None },
            chains: vec![ChainHealthDetail {
                chain: Chain::Ethereum, status: HealthStatus::Healthy,
                block_height: 19_500_000, latency_ms: 45,
                last_scan_at: Some(Utc::now()),
                pending_deposits: 3, pending_withdrawals: 1,
            }],
            pipeline: PipelineHealth {
                received: 10, validated: 8, signed: 7, broadcast: 5,
                confirming: 3, stuck_count: 0,
            },
            background_tasks: vec![TaskHealth {
                name: "eth_scanner".into(), running: true,
                last_run_at: Some(Utc::now()), interval_secs: 12, error_count: 0,
            }],
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("19500000"));
        assert!(json.contains("eth_scanner"));
    }

    #[test]
    fn pipeline_health_stuck_detection() {
        let ph = PipelineHealth {
            received: 100, validated: 95, signed: 90,
            broadcast: 85, confirming: 80, stuck_count: 2,
        };
        assert!(ph.stuck_count > 0);
    }
}
