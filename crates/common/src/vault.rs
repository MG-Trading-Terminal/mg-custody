//! Vault and account abstractions (Fireblocks parity).
//!
//! A Vault is a logical container that groups VaultAccounts.
//! Each VaultAccount represents one (asset, chain) pair with its own derived address.
//! This is the central organizational primitive for institutional custody.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Asset, Chain};

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

/// A vault is a logical container for accounts (Fireblocks core concept).
///
/// Vaults group related asset accounts under a single organizational unit.
/// They can represent a department, a client, or a trading strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    /// Automatically fund deposit addresses with gas (ETH for ERC-20, etc.).
    pub auto_fuel: bool,
    /// Soft-deleted vaults are hidden but retain data.
    pub hidden: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single asset account within a vault.
///
/// One VaultAccount per (vault, asset, chain). Holds a derived blockchain address
/// and tracks cached balances from the double-entry ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultAccount {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub address: String,
    pub derivation_path: String,
    /// Cached balance from the ledger (source of truth is custody_ledger).
    pub balance: Decimal,
    /// In-flight balance (unconfirmed deposits).
    pub pending_balance: Decimal,
    /// Frozen accounts cannot initiate withdrawals.
    pub frozen: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Internal transfers (vault-to-vault, ledger-only)
// ---------------------------------------------------------------------------

/// An instant off-chain transfer between two vaults within the same custody.
///
/// No on-chain transaction — purely a ledger movement. This is one of
/// Fireblocks' key differentiators: zero-fee, instant internal transfers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransfer {
    pub id: Uuid,
    pub source_vault_id: Uuid,
    pub dest_vault_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub amount: Decimal,
    pub status: InternalTransferStatus,
    /// Client-provided idempotency key.
    pub idempotency_key: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternalTransferStatus {
    Pending,
    Completed,
    Failed,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Unified transaction view
// ---------------------------------------------------------------------------

/// Unified transaction record merging deposits, withdrawals, and internal transfers.
///
/// This provides a single view across all transaction types, matching
/// Fireblocks' `/v1/transactions` unified endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub tx_type: TransactionType,
    pub asset: Asset,
    pub chain: Chain,
    pub amount: Decimal,
    pub fee: Option<Decimal>,
    pub status: String,
    pub source_vault_id: Option<Uuid>,
    pub dest_vault_id: Option<Uuid>,
    pub source_address: Option<String>,
    pub dest_address: Option<String>,
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    InternalTransfer,
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deposit => f.write_str("deposit"),
            Self::Withdrawal => f.write_str("withdrawal"),
            Self::InternalTransfer => f.write_str("internal_transfer"),
        }
    }
}

/// Filter for the unified transaction history endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionFilter {
    pub vault_id: Option<Uuid>,
    pub asset: Option<String>,
    pub chain: Option<String>,
    pub status: Option<String>,
    pub tx_type: Option<TransactionType>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub min_amount: Option<Decimal>,
    pub max_amount: Option<Decimal>,
    /// Cursor for pagination (base64-encoded created_at:id).
    pub cursor: Option<String>,
    /// Max results per page (default 50, max 200).
    pub limit: Option<u32>,
}

impl TransactionFilter {
    pub fn effective_limit(&self) -> u32 {
        self.limit.unwrap_or(50).min(200)
    }
}

// ---------------------------------------------------------------------------
// Raw message signing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawSigningAlgorithm {
    Secp256k1,
    Ed25519,
}

/// How the input message should be interpreted before signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageEncoding {
    /// Raw bytes, hex-encoded. Hashed with Keccak-256 (secp256k1) or signed directly (Ed25519).
    Hex,
    /// EIP-191 personal_sign: `"\x19Ethereum Signed Message:\n{len}{message}"`.
    Eip191,
    /// EIP-712 typed data: caller provides the 32-byte struct hash.
    Eip712,
    /// UTF-8 text, hashed with Keccak-256 before signing.
    Utf8,
}

/// Request to sign an arbitrary message with a vault's key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSignRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    /// Hex-encoded message bytes (or EIP-712 hash).
    pub message: String,
    pub encoding: MessageEncoding,
    pub note: Option<String>,
}

/// Response from raw message signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSignResponse {
    pub id: Uuid,
    /// Hex-encoded signature.
    pub signature: String,
    /// Hex-encoded public key.
    pub public_key: String,
    pub algorithm: RawSigningAlgorithm,
    pub signed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Fee estimation (API response)
// ---------------------------------------------------------------------------

/// Multi-level fee estimate response for a chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimateResponse {
    pub chain: Chain,
    pub asset: Asset,
    pub low: FeeLevel,
    pub medium: FeeLevel,
    pub high: FeeLevel,
    pub estimated_at: DateTime<Utc>,
}

/// A single fee level (low/medium/high).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeLevel {
    /// Estimated fee in native token units.
    pub fee: Decimal,
    /// Gas price in gwei (ETH-specific).
    pub gas_price_gwei: Option<Decimal>,
    /// Fee rate in sat/vB (BTC-specific).
    pub sat_per_vbyte: Option<Decimal>,
    /// Estimated time to confirmation in seconds.
    pub estimated_seconds: u32,
}

// ---------------------------------------------------------------------------
// Managed assets (dynamic token registry)
// ---------------------------------------------------------------------------

/// A managed asset/token in the custody system.
///
/// Unlike the hardcoded `TokenConfig`, these are stored in the database
/// and can be added/removed via API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAsset {
    pub id: Uuid,
    pub symbol: String,
    pub chain: Chain,
    pub contract_address: Option<String>,
    pub decimals: u8,
    pub is_native: bool,
    pub enabled: bool,
    pub added_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultRequest {
    pub name: String,
    #[serde(default)]
    pub auto_fuel: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVaultRequest {
    pub name: Option<String>,
    pub auto_fuel: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultAccountRequest {
    pub asset: Asset,
    pub chain: Chain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTransferRequest {
    pub source_vault_id: Uuid,
    pub dest_vault_id: Uuid,
    pub asset: Asset,
    pub chain: Chain,
    pub amount: Decimal,
    pub idempotency_key: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAssetRequest {
    pub symbol: String,
    pub chain: Chain,
    pub contract_address: Option<String>,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAssetRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimateQuery {
    pub chain: Chain,
    pub asset: Option<String>,
    pub amount: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccelerateRequest {
    /// Fee multiplier (default 1.25, max 5.0).
    pub fee_multiplier: Option<Decimal>,
}

// ---------------------------------------------------------------------------
// Vault detail (vault + accounts)
// ---------------------------------------------------------------------------

/// Vault with all its accounts — returned by GET /v1/vaults/{id}.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultDetail {
    #[serde(flatten)]
    pub vault: Vault,
    pub accounts: Vec<VaultAccount>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn vault_serialization_roundtrip() {
        let vault = Vault {
            id: Uuid::nil(),
            name: "Trading Hot".into(),
            owner_id: Uuid::nil(),
            auto_fuel: true,
            hidden: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&vault).unwrap();
        assert!(json.contains("Trading Hot"));
        assert!(json.contains("auto_fuel"));
        let parsed: Vault = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Trading Hot");
        assert!(parsed.auto_fuel);
    }

    #[test]
    fn vault_account_serialization() {
        let account = VaultAccount {
            id: Uuid::nil(),
            vault_id: Uuid::nil(),
            asset: Asset::new("ETH"),
            chain: Chain::Ethereum,
            address: "0xabc".into(),
            derivation_path: "m/44'/60'/0'/0/0".into(),
            balance: dec!(1.5),
            pending_balance: dec!(0.1),
            frozen: false,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains("\"1.5\"") || json.contains("1.5"));
        let parsed: VaultAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.balance, dec!(1.5));
    }

    #[test]
    fn internal_transfer_status_serialization() {
        let json = serde_json::to_string(&InternalTransferStatus::Completed).unwrap();
        assert_eq!(json, r#""completed""#);
    }

    #[test]
    fn transaction_type_display() {
        assert_eq!(TransactionType::Deposit.to_string(), "deposit");
        assert_eq!(TransactionType::Withdrawal.to_string(), "withdrawal");
        assert_eq!(TransactionType::InternalTransfer.to_string(), "internal_transfer");
    }

    #[test]
    fn transaction_filter_effective_limit() {
        let f = TransactionFilter::default();
        assert_eq!(f.effective_limit(), 50);

        let f = TransactionFilter { limit: Some(300), ..Default::default() };
        assert_eq!(f.effective_limit(), 200);

        let f = TransactionFilter { limit: Some(10), ..Default::default() };
        assert_eq!(f.effective_limit(), 10);
    }

    #[test]
    fn message_encoding_serialization() {
        let json = serde_json::to_string(&MessageEncoding::Eip191).unwrap();
        assert_eq!(json, r#""eip191""#);
        let json = serde_json::to_string(&MessageEncoding::Eip712).unwrap();
        assert_eq!(json, r#""eip712""#);
    }

    #[test]
    fn fee_level_serialization() {
        let level = FeeLevel {
            fee: dec!(0.001),
            gas_price_gwei: Some(dec!(30)),
            sat_per_vbyte: None,
            estimated_seconds: 12,
        };
        let json = serde_json::to_string(&level).unwrap();
        assert!(json.contains("estimated_seconds"));
    }

    #[test]
    fn managed_asset_native() {
        let asset = ManagedAsset {
            id: Uuid::nil(),
            symbol: "ETH".into(),
            chain: Chain::Ethereum,
            contract_address: None,
            decimals: 18,
            is_native: true,
            enabled: true,
            added_at: Utc::now(),
        };
        assert!(asset.is_native);
        assert!(asset.contract_address.is_none());
    }

    #[test]
    fn managed_asset_erc20() {
        let asset = ManagedAsset {
            id: Uuid::nil(),
            symbol: "USDT".into(),
            chain: Chain::Ethereum,
            contract_address: Some("0xdAC17F958D2ee523a2206206994597C13D831ec7".into()),
            decimals: 6,
            is_native: false,
            enabled: true,
            added_at: Utc::now(),
        };
        assert!(!asset.is_native);
        assert_eq!(asset.decimals, 6);
    }

    #[test]
    fn create_vault_request_defaults() {
        let json = r#"{"name":"Test Vault"}"#;
        let req: CreateVaultRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Test Vault");
        assert!(!req.auto_fuel);
    }

    #[test]
    fn vault_detail_flattened() {
        let detail = VaultDetail {
            vault: Vault {
                id: Uuid::nil(),
                name: "Main".into(),
                owner_id: Uuid::nil(),
                auto_fuel: false,
                hidden: false,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            accounts: vec![],
        };
        let json = serde_json::to_string(&detail).unwrap();
        // Flattened: name should be at top level, not nested under "vault"
        assert!(json.contains(r#""name":"Main""#));
        assert!(json.contains(r#""accounts":[]"#));
    }

    #[test]
    fn accelerate_request_defaults() {
        let json = r#"{}"#;
        let req: AccelerateRequest = serde_json::from_str(json).unwrap();
        assert!(req.fee_multiplier.is_none());
    }
}
