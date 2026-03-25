//! Webhook management types.
//!
//! Clients register webhook endpoints, configure event filters,
//! and manage delivery settings — matching Fireblocks' webhook system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Webhook endpoint registration
// ---------------------------------------------------------------------------

/// A registered webhook endpoint that receives custody events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub url: String,
    /// Optional HMAC secret for signing payloads sent to this endpoint.
    pub secret: Option<String>,
    /// Event types to deliver (empty = all events).
    pub event_filter: Vec<String>,
    /// Optional chain filter (empty = all chains).
    pub chain_filter: Vec<Chain>,
    pub enabled: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebhookRequest {
    pub url: String,
    /// Event types to subscribe to. Empty = all.
    #[serde(default)]
    pub event_filter: Vec<String>,
    #[serde(default)]
    pub chain_filter: Vec<Chain>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWebhookRequest {
    pub url: Option<String>,
    pub event_filter: Option<Vec<String>>,
    pub chain_filter: Option<Vec<Chain>>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
}

/// Test result from sending a test webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTestResult {
    pub endpoint_id: Uuid,
    pub success: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub tested_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Batch transactions
// ---------------------------------------------------------------------------

/// Request to submit multiple withdrawals in a single batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWithdrawalRequest {
    /// Client-provided batch ID for idempotency.
    pub batch_id: Option<String>,
    pub withdrawals: Vec<BatchWithdrawalItem>,
}

/// A single withdrawal in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWithdrawalItem {
    pub withdrawal_id: Uuid,
    pub asset: crate::Asset,
    pub chain: Chain,
    pub address: String,
    pub amount: rust_decimal::Decimal,
    pub note: Option<String>,
}

/// Response for batch submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWithdrawalResponse {
    pub batch_id: String,
    pub total: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub results: Vec<BatchItemResult>,
}

/// Result for a single item in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItemResult {
    pub withdrawal_id: Uuid,
    pub custody_ref: Option<Uuid>,
    pub status: String,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Address book
// ---------------------------------------------------------------------------

/// A saved destination address with a label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressBookEntry {
    pub id: Uuid,
    pub label: String,
    pub address: String,
    pub chain: Chain,
    pub tag: Option<String>,
    /// Whether this address has been whitelisted for withdrawals.
    pub whitelisted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAddressBookRequest {
    pub label: String,
    pub address: String,
    pub chain: Chain,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAddressBookRequest {
    pub label: Option<String>,
    pub tag: Option<String>,
    pub whitelisted: Option<bool>,
}

// ---------------------------------------------------------------------------
// Audit log query
// ---------------------------------------------------------------------------

/// Filter for querying the audit log via API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditLogFilter {
    pub operation: Option<String>,
    pub actor: Option<String>,
    pub chain: Option<String>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

impl AuditLogFilter {
    pub fn effective_limit(&self) -> u32 {
        self.limit.unwrap_or(50).min(500)
    }
}

// ---------------------------------------------------------------------------
// Gas station status
// ---------------------------------------------------------------------------

/// Gas station status for a vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasStationStatus {
    pub chain: Chain,
    pub enabled: bool,
    pub current_balance: rust_decimal::Decimal,
    pub threshold: rust_decimal::Decimal,
    pub cap: rust_decimal::Decimal,
    pub needs_refuel: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_endpoint_serialization() {
        let ep = WebhookEndpoint {
            id: Uuid::nil(),
            url: "https://example.com/webhook".into(),
            secret: None,
            event_filter: vec!["deposit_confirmed".into(), "withdrawal_broadcast".into()],
            chain_filter: vec![Chain::Ethereum],
            enabled: true,
            description: Some("Main webhook".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(json.contains("deposit_confirmed"));
        assert!(json.contains("ethereum"));
    }

    #[test]
    fn batch_withdrawal_request() {
        let req = BatchWithdrawalRequest {
            batch_id: Some("batch-001".into()),
            withdrawals: vec![
                BatchWithdrawalItem {
                    withdrawal_id: Uuid::new_v4(),
                    asset: crate::Asset::new("ETH"),
                    chain: Chain::Ethereum,
                    address: "0x1234".into(),
                    amount: rust_decimal::Decimal::new(1, 0),
                    note: None,
                },
            ],
        };
        assert_eq!(req.withdrawals.len(), 1);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("batch-001"));
    }

    #[test]
    fn address_book_entry_serialization() {
        let entry = AddressBookEntry {
            id: Uuid::nil(),
            label: "Binance Hot".into(),
            address: "0xabc123".into(),
            chain: Chain::Ethereum,
            tag: Some("exchange".into()),
            whitelisted: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Binance Hot"));
        assert!(json.contains("whitelisted"));
    }

    #[test]
    fn audit_log_filter_effective_limit() {
        let f = AuditLogFilter::default();
        assert_eq!(f.effective_limit(), 50);
        let f = AuditLogFilter { limit: Some(1000), ..Default::default() };
        assert_eq!(f.effective_limit(), 500);
    }

    #[test]
    fn create_webhook_request_defaults() {
        let json = r#"{"url":"https://example.com/hook"}"#;
        let req: CreateWebhookRequest = serde_json::from_str(json).unwrap();
        assert!(req.event_filter.is_empty());
        assert!(req.chain_filter.is_empty());
    }

    #[test]
    fn batch_response_structure() {
        let resp = BatchWithdrawalResponse {
            batch_id: "b-001".into(),
            total: 3,
            accepted: 2,
            rejected: 1,
            results: vec![
                BatchItemResult {
                    withdrawal_id: Uuid::nil(),
                    custody_ref: Some(Uuid::nil()),
                    status: "accepted".into(),
                    error: None,
                },
                BatchItemResult {
                    withdrawal_id: Uuid::nil(),
                    custody_ref: None,
                    status: "rejected".into(),
                    error: Some("Invalid address".into()),
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("rejected"));
        assert!(json.contains("Invalid address"));
    }

    #[test]
    fn gas_station_status() {
        let status = GasStationStatus {
            chain: Chain::Ethereum,
            enabled: true,
            current_balance: rust_decimal::Decimal::new(3, 2), // 0.03
            threshold: rust_decimal::Decimal::new(1, 2),       // 0.01
            cap: rust_decimal::Decimal::new(5, 2),             // 0.05
            needs_refuel: false,
        };
        assert!(!status.needs_refuel);
    }
}
