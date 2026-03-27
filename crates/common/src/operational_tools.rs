//! Operational tools — webhook resend, NFT spam, audit search, config backup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Webhook resend request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResendRequest {
    pub delivery_id: Uuid,
    pub force: bool,
}

/// NFT spam detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftSpamFlag {
    pub contract_address: String,
    pub chain: String,
    pub token_id: Option<String>,
    pub is_spam: bool,
    pub reason: Option<SpamReason>,
    pub flagged_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpamReason { AirdropScam, PhishingLink, CopycatCollection, SuspiciousMetadata, UserReported }

/// Audit log search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSearchQuery {
    pub query: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub limit: u32,
    pub offset: u32,
}

/// Config export for backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigExport {
    pub version: String,
    pub exported_at: DateTime<Utc>,
    pub workspace_settings: serde_json::Value,
    pub policy_rules: Vec<serde_json::Value>,
    pub whitelist_entries: Vec<serde_json::Value>,
    pub webhook_endpoints: Vec<serde_json::Value>,
    pub gas_station_configs: Vec<serde_json::Value>,
    pub vault_policies: Vec<serde_json::Value>,
}

/// Config import result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigImportResult {
    pub success: bool,
    pub items_imported: u32,
    pub items_skipped: u32,
    pub errors: Vec<String>,
}

/// System health aggregation across all components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemReadiness {
    pub overall: ReadinessLevel,
    pub database: ReadinessLevel,
    pub signer: ReadinessLevel,
    pub chains: Vec<ChainReadiness>,
    pub webhooks: ReadinessLevel,
    pub compliance: ReadinessLevel,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessLevel { Ready, Degraded, Unavailable, Unknown }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainReadiness {
    pub chain: String,
    pub scanner: ReadinessLevel,
    pub broadcaster: ReadinessLevel,
    pub rpc_latency_ms: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spam_reason_serialization() {
        assert_eq!(serde_json::to_string(&SpamReason::PhishingLink).unwrap(), r#""phishing_link""#);
        assert_eq!(serde_json::to_string(&SpamReason::AirdropScam).unwrap(), r#""airdrop_scam""#);
    }

    #[test]
    fn readiness_levels() {
        assert_eq!(serde_json::to_string(&ReadinessLevel::Degraded).unwrap(), r#""degraded""#);
    }

    #[test]
    fn config_export_structure() {
        let export = ConfigExport {
            version: "1.0".into(), exported_at: Utc::now(),
            workspace_settings: serde_json::json!({}),
            policy_rules: vec![], whitelist_entries: vec![],
            webhook_endpoints: vec![], gas_station_configs: vec![],
            vault_policies: vec![],
        };
        let json = serde_json::to_string(&export).unwrap();
        assert!(json.contains("version"));
    }

    #[test]
    fn audit_search_query() {
        let q = AuditSearchQuery {
            query: Some("withdrawal".into()), actor: None, action: None,
            resource_type: None, after: None, before: None, limit: 100, offset: 0,
        };
        assert_eq!(q.limit, 100);
    }

    #[test]
    fn system_readiness() {
        let readiness = SystemReadiness {
            overall: ReadinessLevel::Ready,
            database: ReadinessLevel::Ready,
            signer: ReadinessLevel::Ready,
            chains: vec![ChainReadiness {
                chain: "ethereum".into(), scanner: ReadinessLevel::Ready,
                broadcaster: ReadinessLevel::Ready, rpc_latency_ms: Some(45),
            }],
            webhooks: ReadinessLevel::Ready,
            compliance: ReadinessLevel::Degraded,
            checked_at: Utc::now(),
        };
        assert_eq!(readiness.overall, ReadinessLevel::Ready);
    }
}
