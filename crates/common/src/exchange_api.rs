//! Exchange API clients — connect to Binance, Coinbase, Kraken etc.
//!
//! Used for: balance monitoring, withdrawal routing, auto-rebalancing.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported exchange integrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exchange {
    Binance,
    Coinbase,
    CoinbasePrime,
    Kraken,
    Okx,
    Bybit,
    Deribit,
    Bitfinex,
    Custom,
}

/// Exchange account connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeAccount {
    pub id: Uuid,
    pub exchange: Exchange,
    pub name: String,
    pub api_key_id: String,
    pub sub_accounts: Vec<SubAccount>,
    pub status: ExchangeAccountStatus,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeAccountStatus { Connected, Disconnected, RateLimited, Error }

/// Exchange sub-account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAccount {
    pub id: String,
    pub name: String,
    pub account_type: SubAccountType,
    pub balances: Vec<ExchangeBalance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubAccountType { Spot, Margin, Futures, Funding, Earn }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
    pub total: Decimal,
}

/// Internal transfer between exchange sub-accounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeInternalTransfer {
    pub from_account: String,
    pub to_account: String,
    pub asset: String,
    pub amount: Decimal,
}

/// Off-exchange settlement (Fireblocks Network equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffExchangeSettlement {
    pub id: Uuid,
    pub counterparty_id: Uuid,
    pub asset: String,
    pub amount: Decimal,
    pub direction: SettlementDirection,
    pub status: OffExchangeStatus,
    pub collateral_locked: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementDirection { Deliver, Receive }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffExchangeStatus { Pending, CollateralLocked, Settling, Settled, Failed }

/// Network ID for Fireblocks Network equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkId {
    pub id: Uuid,
    pub name: String,
    pub is_discoverable: bool,
    pub routing_policy_id: Option<Uuid>,
    pub connected_peers: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Webhook V2 delivery metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDeliveryMetrics {
    pub endpoint_id: Uuid,
    pub total_deliveries: u64,
    pub successful: u64,
    pub failed: u64,
    pub avg_latency_ms: f32,
    pub p99_latency_ms: f32,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
}

/// Webhook event types (expanded from V1).
pub mod webhook_events {
    pub const TRANSACTION_CREATED: &str = "transaction.created";
    pub const TRANSACTION_STATUS_UPDATED: &str = "transaction.status_updated";
    pub const TRANSACTION_APPROVAL_CHANGED: &str = "transaction.approval_status_changed";
    pub const VAULT_ACCOUNT_ADDED: &str = "vault_account.added";
    pub const VAULT_ACCOUNT_ASSET_ADDED: &str = "vault_account.asset_added";
    pub const EXTERNAL_WALLET_ADDED: &str = "external_wallet.asset_added";
    pub const INTERNAL_WALLET_ADDED: &str = "internal_wallet.asset_added";
    pub const NETWORK_CONNECTION: &str = "network_connection.created";
    pub const SMART_TRANSFER_TICKET: &str = "smart_transfer.ticket_created";
    pub const TOKENIZATION_MINT: &str = "tokenization.mint";
    pub const TOKENIZATION_BURN: &str = "tokenization.burn";
    pub const OFF_EXCHANGE_SETTLED: &str = "off_exchange.settled";
    pub const STAKING_POSITION_CHANGED: &str = "staking.position_changed";
    pub const CONFIG_CHANGE: &str = "config.changed";
    pub const POLICY_VIOLATION: &str = "policy.violation";
}

/// All webhook event types.
pub fn all_webhook_event_types() -> Vec<&'static str> {
    vec![
        webhook_events::TRANSACTION_CREATED,
        webhook_events::TRANSACTION_STATUS_UPDATED,
        webhook_events::TRANSACTION_APPROVAL_CHANGED,
        webhook_events::VAULT_ACCOUNT_ADDED,
        webhook_events::VAULT_ACCOUNT_ASSET_ADDED,
        webhook_events::EXTERNAL_WALLET_ADDED,
        webhook_events::INTERNAL_WALLET_ADDED,
        webhook_events::NETWORK_CONNECTION,
        webhook_events::SMART_TRANSFER_TICKET,
        webhook_events::TOKENIZATION_MINT,
        webhook_events::TOKENIZATION_BURN,
        webhook_events::OFF_EXCHANGE_SETTLED,
        webhook_events::STAKING_POSITION_CHANGED,
        webhook_events::CONFIG_CHANGE,
        webhook_events::POLICY_VIOLATION,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn exchange_serialization() {
        assert_eq!(serde_json::to_string(&Exchange::CoinbasePrime).unwrap(), r#""coinbase_prime""#);
        assert_eq!(serde_json::to_string(&Exchange::Binance).unwrap(), r#""binance""#);
    }

    #[test]
    fn sub_account_type() {
        assert_eq!(serde_json::to_string(&SubAccountType::Futures).unwrap(), r#""futures""#);
    }

    #[test]
    fn exchange_balance() {
        let bal = ExchangeBalance { asset: "BTC".into(), free: dec!(1.5), locked: dec!(0.5), total: dec!(2.0) };
        let json = serde_json::to_string(&bal).unwrap();
        assert!(json.contains("1.5"));
    }

    #[test]
    fn off_exchange_settlement() {
        let s = OffExchangeSettlement {
            id: Uuid::nil(), counterparty_id: Uuid::nil(),
            asset: "USDC".into(), amount: dec!(1000000),
            direction: SettlementDirection::Deliver,
            status: OffExchangeStatus::CollateralLocked,
            collateral_locked: Some(dec!(1050000)),
            created_at: Utc::now(), settled_at: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("collateral_locked"));
    }

    #[test]
    fn webhook_event_types_count() {
        assert!(all_webhook_event_types().len() >= 15);
    }

    #[test]
    fn webhook_delivery_metrics() {
        let m = WebhookDeliveryMetrics {
            endpoint_id: Uuid::nil(), total_deliveries: 10000,
            successful: 9950, failed: 50,
            avg_latency_ms: 45.0, p99_latency_ms: 200.0,
            last_delivery_at: Some(Utc::now()), last_failure_at: None,
        };
        assert_eq!(m.successful + m.failed, m.total_deliveries);
    }

    #[test]
    fn network_id_structure() {
        let nid = NetworkId {
            id: Uuid::nil(), name: "MG Custody Node 1".into(),
            is_discoverable: true, routing_policy_id: None,
            connected_peers: vec![], created_at: Utc::now(),
        };
        assert!(nid.is_discoverable);
    }

    #[test]
    fn exchange_account_status() {
        assert_eq!(serde_json::to_string(&ExchangeAccountStatus::RateLimited).unwrap(), r#""rate_limited""#);
    }
}
