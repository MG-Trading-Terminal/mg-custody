//! Connectivity & integration types.
//!
//! Exchange connections, API key management, notification channels,
//! and system configuration API.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Exchange / counterparty connections
// ---------------------------------------------------------------------------

/// A connection to an external exchange or counterparty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConnection {
    pub id: Uuid,
    pub name: String,
    pub connection_type: ConnectionType,
    pub status: ConnectionStatus,
    /// Base URL for the exchange API.
    pub api_url: Option<String>,
    /// Supported chains for this connection.
    pub supported_chains: Vec<Chain>,
    /// Whether deposits from this exchange are auto-credited.
    pub auto_credit: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Exchange,
    OtcDesk,
    Custodian,
    InternalVault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    Inactive,
    Pending,
    Suspended,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Inactive => f.write_str("inactive"),
            Self::Pending => f.write_str("pending"),
            Self::Suspended => f.write_str("suspended"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConnectionRequest {
    pub name: String,
    pub connection_type: ConnectionType,
    pub api_url: Option<String>,
    #[serde(default)]
    pub supported_chains: Vec<Chain>,
    #[serde(default)]
    pub auto_credit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConnectionRequest {
    pub name: Option<String>,
    pub status: Option<ConnectionStatus>,
    pub api_url: Option<String>,
    pub supported_chains: Option<Vec<Chain>>,
    pub auto_credit: Option<bool>,
}

// ---------------------------------------------------------------------------
// API key management
// ---------------------------------------------------------------------------

/// A managed API key with scoped permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    /// The key value — only shown on creation, then stored as hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// SHA-256 hash of the key (stored in DB, used for lookup).
    pub key_hash: String,
    pub role: ApiKeyRole,
    /// Allowed permissions for this key.
    pub permissions: Vec<ApiPermission>,
    pub ip_whitelist: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyRole {
    Admin,
    Signer,
    Viewer,
    Trader,
}

impl std::fmt::Display for ApiKeyRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admin => f.write_str("admin"),
            Self::Signer => f.write_str("signer"),
            Self::Viewer => f.write_str("viewer"),
            Self::Trader => f.write_str("trader"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiPermission {
    VaultRead,
    VaultWrite,
    TransactionRead,
    TransactionWrite,
    TransactionSign,
    AddressBookRead,
    AddressBookWrite,
    WebhookManage,
    AuditRead,
    AdminAll,
}

impl std::fmt::Display for ApiPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_default();
        f.write_str(s.trim_matches('"'))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub role: ApiKeyRole,
    #[serde(default)]
    pub permissions: Vec<ApiPermission>,
    #[serde(default)]
    pub ip_whitelist: Vec<String>,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_rate_limit() -> u32 {
    100
}

/// Response when creating an API key — the only time the raw key is returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    /// The raw API key — store securely, cannot be retrieved again.
    pub api_key: String,
    pub role: ApiKeyRole,
    pub permissions: Vec<ApiPermission>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Notification channels
// ---------------------------------------------------------------------------

/// A configured notification channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub name: String,
    pub channel_type: ChannelType,
    pub config: ChannelConfig,
    /// Minimum severity to notify on this channel.
    pub min_severity: crate::alerts::AlertSeverity,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Telegram,
    Slack,
    Email,
    Webhook,
    PagerDuty,
}

/// Channel-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelConfig {
    Telegram {
        bot_token: String,
        chat_id: String,
    },
    Slack {
        webhook_url: String,
        channel: Option<String>,
    },
    Email {
        smtp_host: String,
        smtp_port: u16,
        from_address: String,
        to_addresses: Vec<String>,
    },
    Webhook {
        url: String,
        secret: Option<String>,
    },
    PagerDuty {
        integration_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: ChannelType,
    pub config: ChannelConfig,
    pub min_severity: crate::alerts::AlertSeverity,
}

// ---------------------------------------------------------------------------
// System configuration
// ---------------------------------------------------------------------------

/// Runtime configuration that can be modified via API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Per-transaction withdrawal limit (USD).
    pub withdrawal_limit_single: Decimal,
    /// Hourly outflow limit (USD).
    pub withdrawal_limit_hourly: Decimal,
    /// Daily outflow limit (USD).
    pub withdrawal_limit_daily: Decimal,
    /// Global pause — all withdrawals halted.
    pub withdrawals_paused: bool,
    /// Global pause — all deposits halted.
    pub deposits_paused: bool,
    /// Minimum confirmations override per chain.
    pub confirmation_overrides: Vec<ConfirmationOverride>,
    /// Maintenance mode — API returns 503.
    pub maintenance_mode: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            withdrawal_limit_single: Decimal::new(500_000, 0),
            withdrawal_limit_hourly: Decimal::new(10_000_000, 0),
            withdrawal_limit_daily: Decimal::new(50_000_000, 0),
            withdrawals_paused: false,
            deposits_paused: false,
            confirmation_overrides: vec![],
            maintenance_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationOverride {
    pub chain: Chain,
    pub confirmations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSystemConfigRequest {
    pub withdrawal_limit_single: Option<Decimal>,
    pub withdrawal_limit_hourly: Option<Decimal>,
    pub withdrawal_limit_daily: Option<Decimal>,
    pub withdrawals_paused: Option<bool>,
    pub deposits_paused: Option<bool>,
    pub confirmation_overrides: Option<Vec<ConfirmationOverride>>,
    pub maintenance_mode: Option<bool>,
}

// ---------------------------------------------------------------------------
// Network fee cache (real-time from RPC)
// ---------------------------------------------------------------------------

/// Cached network fee data from RPC nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFeeCache {
    pub chain: Chain,
    pub base_fee: Decimal,
    pub priority_fee: Decimal,
    pub max_fee: Decimal,
    pub last_updated: DateTime<Utc>,
    pub block_number: Option<u64>,
    pub is_stale: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn connection_type_serialization() {
        let json = serde_json::to_string(&ConnectionType::Exchange).unwrap();
        assert_eq!(json, r#""exchange""#);
        let json = serde_json::to_string(&ConnectionType::OtcDesk).unwrap();
        assert_eq!(json, r#""otc_desk""#);
    }

    #[test]
    fn connection_status_display() {
        assert_eq!(ConnectionStatus::Active.to_string(), "active");
        assert_eq!(ConnectionStatus::Suspended.to_string(), "suspended");
    }

    #[test]
    fn api_key_role_serialization() {
        let json = serde_json::to_string(&ApiKeyRole::Admin).unwrap();
        assert_eq!(json, r#""admin""#);
        let json = serde_json::to_string(&ApiKeyRole::Trader).unwrap();
        assert_eq!(json, r#""trader""#);
    }

    #[test]
    fn api_permission_all_variants() {
        let perms = vec![
            ApiPermission::VaultRead, ApiPermission::VaultWrite,
            ApiPermission::TransactionRead, ApiPermission::TransactionWrite,
            ApiPermission::TransactionSign,
            ApiPermission::AddressBookRead, ApiPermission::AddressBookWrite,
            ApiPermission::WebhookManage, ApiPermission::AuditRead,
            ApiPermission::AdminAll,
        ];
        for p in &perms {
            let json = serde_json::to_string(p).unwrap();
            let parsed: ApiPermission = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, parsed);
        }
        assert_eq!(perms.len(), 10);
    }

    #[test]
    fn create_api_key_request_defaults() {
        let json = r#"{"name":"Test Key","role":"viewer"}"#;
        let req: CreateApiKeyRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.rate_limit_per_minute, 100);
        assert!(req.permissions.is_empty());
        assert!(req.ip_whitelist.is_empty());
    }

    #[test]
    fn channel_config_telegram() {
        let config = ChannelConfig::Telegram {
            bot_token: "123:ABC".into(),
            chat_id: "-100123".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("telegram"));
        assert!(json.contains("bot_token"));
    }

    #[test]
    fn channel_config_slack() {
        let config = ChannelConfig::Slack {
            webhook_url: "https://hooks.slack.com/services/T/B/x".into(),
            channel: Some("#alerts".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("slack"));
        assert!(json.contains("#alerts"));
    }

    #[test]
    fn channel_config_pagerduty() {
        let config = ChannelConfig::PagerDuty {
            integration_key: "key123".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("pager_duty"));
    }

    #[test]
    fn system_config_defaults() {
        let config = SystemConfig::default();
        assert_eq!(config.withdrawal_limit_single, dec!(500000));
        assert_eq!(config.withdrawal_limit_hourly, dec!(10000000));
        assert_eq!(config.withdrawal_limit_daily, dec!(50000000));
        assert!(!config.withdrawals_paused);
        assert!(!config.maintenance_mode);
    }

    #[test]
    fn system_config_serialization_roundtrip() {
        let config = SystemConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SystemConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.withdrawal_limit_single, config.withdrawal_limit_single);
    }

    #[test]
    fn network_fee_cache() {
        let fee = NetworkFeeCache {
            chain: Chain::Ethereum,
            base_fee: dec!(25.5),
            priority_fee: dec!(2.0),
            max_fee: dec!(53.0),
            last_updated: Utc::now(),
            block_number: Some(19_000_000),
            is_stale: false,
        };
        let json = serde_json::to_string(&fee).unwrap();
        assert!(json.contains("19000000"));
    }

    #[test]
    fn confirmation_override() {
        let ov = ConfirmationOverride {
            chain: Chain::Bitcoin,
            confirmations: 6,
        };
        let json = serde_json::to_string(&ov).unwrap();
        assert!(json.contains("bitcoin"));
        assert!(json.contains("6"));
    }

    #[test]
    fn notification_channel_serialization() {
        let ch = NotificationChannel {
            id: Uuid::nil(),
            name: "Ops Slack".into(),
            channel_type: ChannelType::Slack,
            config: ChannelConfig::Slack {
                webhook_url: "https://hooks.slack.com/test".into(),
                channel: None,
            },
            min_severity: crate::alerts::AlertSeverity::Warning,
            enabled: true,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains("Ops Slack"));
        assert!(json.contains("warning"));
    }
}
