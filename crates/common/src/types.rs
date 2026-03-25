//! Core types for the custody service.
//!
//! All monetary amounts use `rust_decimal::Decimal` — NEVER f64.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Chain & Asset identifiers
// ---------------------------------------------------------------------------

/// Supported blockchain networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Bitcoin,
    Ethereum,
    Tron,
    Solana,
}

impl Chain {
    /// BIP-44 coin type for HD derivation.
    pub fn coin_type(&self) -> u32 {
        match self {
            Chain::Bitcoin => 0,
            Chain::Ethereum => 60,
            Chain::Tron => 195,
            Chain::Solana => 501,
        }
    }

    /// Minimum confirmations before crediting a deposit.
    pub fn required_confirmations(&self) -> u64 {
        match self {
            Chain::Bitcoin => 3,
            Chain::Ethereum => 12,
            Chain::Tron => 20,
            Chain::Solana => 1, // "finalized" commitment (~32 slots)
        }
    }

    /// Display name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Chain::Bitcoin => "bitcoin",
            Chain::Ethereum => "ethereum",
            Chain::Tron => "tron",
            Chain::Solana => "solana",
        }
    }

    /// Parse from string.
    pub fn parse_str(s: &str) -> Self {
        match s {
            "bitcoin" | "btc" => Chain::Bitcoin,
            "ethereum" | "eth" => Chain::Ethereum,
            "tron" | "trx" => Chain::Tron,
            "solana" | "sol" => Chain::Solana,
            _ => Chain::Ethereum,
        }
    }
}

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Asset identifier (e.g., "BTC", "ETH", "USDT").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset(pub String);

impl Asset {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Token contract configuration for ERC-20 / TRC-20 / SPL tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub asset: Asset,
    pub chain: Chain,
    pub contract_address: String,
    pub decimals: u8,
}

// ---------------------------------------------------------------------------
// Wallet tiers
// ---------------------------------------------------------------------------

/// Wallet security tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WalletTier {
    Hot,
    Warm,
    Cold,
}

// ---------------------------------------------------------------------------
// Deposit types
// ---------------------------------------------------------------------------

/// Status of a detected deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositStatus {
    /// Transaction seen on-chain, waiting for confirmations.
    Detected,
    /// Enough confirmations, waiting for exchange credit webhook.
    Confirmed,
    /// Exchange acknowledged the credit.
    Credited,
    /// Something went wrong (reorg, invalid tx).
    Failed,
}

/// A deposit detected by the chain scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub id: Uuid,
    pub user_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub address: String,
    pub amount: Decimal,
    pub tx_hash: String,
    /// Log index within the transaction (for ERC-20/TRC-20 — one tx can have multiple transfers).
    pub log_index: Option<u32>,
    pub block_height: u64,
    pub confirmations: u64,
    pub required_confirmations: u64,
    pub status: DepositStatus,
    pub detected_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub credited_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Withdrawal types
// ---------------------------------------------------------------------------

/// Status of a withdrawal through the signing pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalStatus {
    /// Received from exchange, pending validation.
    Received,
    /// Passed validation, queued for signing.
    Validated,
    /// Waiting for manual approval (large amount).
    PendingApproval,
    /// Approved, queued for signing.
    Approved,
    /// Transaction signed.
    Signed,
    /// Transaction broadcast to the network.
    Broadcast,
    /// Waiting for on-chain confirmations.
    Confirming,
    /// Confirmed on-chain — done.
    Finalized,
    /// Failed after retries.
    Failed,
    /// Cancelled by admin or user (before broadcast).
    Cancelled,
}

/// A withdrawal request from the exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub id: Uuid,
    /// Exchange's withdrawal UUID (idempotency key).
    pub exchange_withdrawal_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub destination_address: String,
    pub amount: Decimal,
    pub fee: Option<Decimal>,
    pub status: WithdrawalStatus,
    pub wallet_tier: WalletTier,
    pub tx_hash: Option<String>,
    pub block_height: Option<u64>,
    pub confirmations: u64,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub signed_at: Option<DateTime<Utc>>,
    pub broadcast_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    /// Time-delay release (large withdrawals must wait).
    pub delay_release_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Deposit address
// ---------------------------------------------------------------------------

/// A generated deposit address for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositAddress {
    pub id: Uuid,
    pub user_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub address: String,
    pub derivation_path: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Wallet balance
// ---------------------------------------------------------------------------

/// Balance summary for a single asset across wallet tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalance {
    pub asset: Asset,
    pub hot_balance: Decimal,
    pub warm_balance: Decimal,
    pub cold_balance: Decimal,
    pub total: Decimal,
}

// ---------------------------------------------------------------------------
// Webhook events (custody -> exchange)
// ---------------------------------------------------------------------------

/// Events sent from custody service to the exchange via webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum CustodyEvent {
    DepositConfirmed {
        deposit_id: Uuid,
        user_address: String,
        user_id: Uuid,
        asset: Asset,
        chain: Chain,
        amount: Decimal,
        tx_hash: String,
        confirmations: u64,
        block_height: u64,
    },
    WithdrawalBroadcast {
        withdrawal_id: Uuid,
        exchange_withdrawal_id: Uuid,
        tx_hash: String,
        asset: Asset,
        chain: Chain,
        amount: Decimal,
        fee: Decimal,
    },
    WithdrawalConfirmed {
        withdrawal_id: Uuid,
        exchange_withdrawal_id: Uuid,
        tx_hash: String,
        block_height: u64,
        confirmations: u64,
    },
    WithdrawalFailed {
        withdrawal_id: Uuid,
        exchange_withdrawal_id: Uuid,
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

/// Request to create a deposit address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAddressRequest {
    pub user_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
}

/// Response with the generated address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAddressResponse {
    pub address: String,
    pub chain: Chain,
    pub memo: Option<String>,
}

/// Request to submit a withdrawal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitWithdrawalRequest {
    /// Exchange's withdrawal UUID — idempotency key.
    pub withdrawal_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub address: String,
    /// String-encoded decimal — never float.
    pub amount: Decimal,
}

/// Response acknowledging a withdrawal submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitWithdrawalResponse {
    pub custody_ref: Uuid,
    pub status: WithdrawalStatus,
}

/// Response for withdrawal status query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalStatusResponse {
    pub custody_ref: Uuid,
    pub exchange_withdrawal_id: Uuid,
    pub status: WithdrawalStatus,
    pub tx_hash: Option<String>,
    pub confirmations: u64,
    pub required_confirmations: u64,
    pub error: Option<String>,
}

/// Response for wallet balance query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    pub asset: Asset,
    pub hot_balance: Decimal,
    pub warm_balance: Decimal,
    pub total_managed: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_types_match_bip44() {
        assert_eq!(Chain::Bitcoin.coin_type(), 0);
        assert_eq!(Chain::Ethereum.coin_type(), 60);
        assert_eq!(Chain::Tron.coin_type(), 195);
        assert_eq!(Chain::Solana.coin_type(), 501);
    }

    #[test]
    fn chain_serialization() {
        let json = serde_json::to_string(&Chain::Bitcoin).unwrap();
        assert_eq!(json, r#""bitcoin""#);
        let parsed: Chain = serde_json::from_str(r#""ethereum""#).unwrap();
        assert_eq!(parsed, Chain::Ethereum);
    }

    #[test]
    fn deposit_status_serialization() {
        let json = serde_json::to_string(&DepositStatus::Confirmed).unwrap();
        assert_eq!(json, r#""confirmed""#);
    }

    #[test]
    fn withdrawal_status_serialization() {
        let json = serde_json::to_string(&WithdrawalStatus::PendingApproval).unwrap();
        assert_eq!(json, r#""pending_approval""#);
    }

    #[test]
    fn custody_event_tagged_serialization() {
        let event = CustodyEvent::WithdrawalFailed {
            withdrawal_id: Uuid::nil(),
            exchange_withdrawal_id: Uuid::nil(),
            reason: "out of gas".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"withdrawal_failed\""));
        assert!(json.contains("out of gas"));
    }
}
