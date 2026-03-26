//! Accounting types — balance sheets, P&L, fee tracking, ledger reports.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Asset, Chain};

// ---------------------------------------------------------------------------
// Balance sheet
// ---------------------------------------------------------------------------

/// Point-in-time balance sheet for the custody service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSheet {
    pub timestamp: DateTime<Utc>,
    pub assets: Vec<AssetBalance>,
    pub total_aum_usd: Option<Decimal>,
}

/// Balance for a single asset across all tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: Asset,
    pub chain: Chain,
    pub hot_balance: Decimal,
    pub warm_balance: Decimal,
    pub cold_balance: Decimal,
    pub total: Decimal,
    pub pending_deposits: Decimal,
    pub pending_withdrawals: Decimal,
    pub available: Decimal,
}

impl AssetBalance {
    pub fn new(asset: Asset, chain: Chain) -> Self {
        Self {
            asset, chain,
            hot_balance: Decimal::ZERO, warm_balance: Decimal::ZERO,
            cold_balance: Decimal::ZERO, total: Decimal::ZERO,
            pending_deposits: Decimal::ZERO, pending_withdrawals: Decimal::ZERO,
            available: Decimal::ZERO,
        }
    }

    pub fn compute_total(&mut self) {
        self.total = self.hot_balance + self.warm_balance + self.cold_balance;
        self.available = self.total - self.pending_withdrawals;
    }
}

// ---------------------------------------------------------------------------
// Fee tracking
// ---------------------------------------------------------------------------

/// Aggregated fee summary for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeSummary {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub chain: Chain,
    pub total_fees_native: Decimal,
    pub total_fees_usd: Option<Decimal>,
    pub transaction_count: u32,
    pub avg_fee_native: Decimal,
    pub max_fee_native: Decimal,
    pub min_fee_native: Decimal,
}

/// Individual fee record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRecord {
    pub transaction_id: Uuid,
    pub chain: Chain,
    pub fee_native: Decimal,
    pub fee_usd: Option<Decimal>,
    pub gas_used: Option<u64>,
    pub gas_price_gwei: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// P&L (Profit & Loss from custody fees)
// ---------------------------------------------------------------------------

/// Custody P&L for a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodyPnl {
    pub period: String,
    pub withdrawal_fees_collected: Decimal,
    pub network_fees_paid: Decimal,
    pub gas_station_costs: Decimal,
    pub net_fee_income: Decimal,
    pub transaction_count: u32,
}

impl CustodyPnl {
    pub fn calculate(
        withdrawal_fees: Decimal,
        network_fees: Decimal,
        gas_costs: Decimal,
        tx_count: u32,
    ) -> Self {
        Self {
            period: String::new(),
            withdrawal_fees_collected: withdrawal_fees,
            network_fees_paid: network_fees,
            gas_station_costs: gas_costs,
            net_fee_income: withdrawal_fees - network_fees - gas_costs,
            transaction_count: tx_count,
        }
    }
}

// ---------------------------------------------------------------------------
// Event catalog
// ---------------------------------------------------------------------------

/// All custody event types with their schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Deposits
    DepositDetected,
    DepositConfirmed,
    DepositCredited,
    DepositFailed,
    // Withdrawals
    WithdrawalReceived,
    WithdrawalValidated,
    WithdrawalPendingApproval,
    WithdrawalApproved,
    WithdrawalSigned,
    WithdrawalBroadcast,
    WithdrawalConfirmed,
    WithdrawalFailed,
    WithdrawalCancelled,
    // Transfers
    InternalTransferCompleted,
    // Vault
    VaultCreated,
    VaultUpdated,
    VaultFrozen,
    AccountCreated,
    // Policy
    PolicyViolation,
    ApprovalRequired,
    ApprovalDecision,
    // Compliance
    SanctionedAddressDetected,
    HighRiskTransaction,
    TravelRuleRequired,
    // System
    CircuitBreakerTripped,
    CircuitBreakerReset,
    MaintenanceModeEnabled,
    MaintenanceModeDisabled,
    // Key management
    KeyRotationStarted,
    KeyRotationCompleted,
    BackupCreated,
}

impl EventType {
    pub fn category(&self) -> &'static str {
        match self {
            Self::DepositDetected | Self::DepositConfirmed |
            Self::DepositCredited | Self::DepositFailed => "deposit",

            Self::WithdrawalReceived | Self::WithdrawalValidated |
            Self::WithdrawalPendingApproval | Self::WithdrawalApproved |
            Self::WithdrawalSigned | Self::WithdrawalBroadcast |
            Self::WithdrawalConfirmed | Self::WithdrawalFailed |
            Self::WithdrawalCancelled => "withdrawal",

            Self::InternalTransferCompleted => "transfer",

            Self::VaultCreated | Self::VaultUpdated |
            Self::VaultFrozen | Self::AccountCreated => "vault",

            Self::PolicyViolation | Self::ApprovalRequired |
            Self::ApprovalDecision => "policy",

            Self::SanctionedAddressDetected | Self::HighRiskTransaction |
            Self::TravelRuleRequired => "compliance",

            Self::CircuitBreakerTripped | Self::CircuitBreakerReset |
            Self::MaintenanceModeEnabled | Self::MaintenanceModeDisabled => "system",

            Self::KeyRotationStarted | Self::KeyRotationCompleted |
            Self::BackupCreated => "key_management",
        }
    }

    pub fn severity(&self) -> &'static str {
        match self {
            Self::SanctionedAddressDetected | Self::CircuitBreakerTripped => "critical",
            Self::HighRiskTransaction | Self::PolicyViolation |
            Self::WithdrawalFailed | Self::DepositFailed => "warning",
            _ => "info",
        }
    }
}

/// List all event types.
pub fn all_event_types() -> Vec<EventType> {
    vec![
        EventType::DepositDetected, EventType::DepositConfirmed,
        EventType::DepositCredited, EventType::DepositFailed,
        EventType::WithdrawalReceived, EventType::WithdrawalValidated,
        EventType::WithdrawalPendingApproval, EventType::WithdrawalApproved,
        EventType::WithdrawalSigned, EventType::WithdrawalBroadcast,
        EventType::WithdrawalConfirmed, EventType::WithdrawalFailed,
        EventType::WithdrawalCancelled, EventType::InternalTransferCompleted,
        EventType::VaultCreated, EventType::VaultUpdated,
        EventType::VaultFrozen, EventType::AccountCreated,
        EventType::PolicyViolation, EventType::ApprovalRequired,
        EventType::ApprovalDecision, EventType::SanctionedAddressDetected,
        EventType::HighRiskTransaction, EventType::TravelRuleRequired,
        EventType::CircuitBreakerTripped, EventType::CircuitBreakerReset,
        EventType::MaintenanceModeEnabled, EventType::MaintenanceModeDisabled,
        EventType::KeyRotationStarted, EventType::KeyRotationCompleted,
        EventType::BackupCreated,
    ]
}

// ---------------------------------------------------------------------------
// Token bucket rate limiter
// ---------------------------------------------------------------------------

/// Token bucket rate limiter state.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u32,
    pub tokens: f64,
    pub refill_rate: f64, // tokens per second
    pub last_refill: std::time::Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_rate_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate: refill_rate_per_sec,
            last_refill: std::time::Instant::now(),
        }
    }

    /// Try to consume one token. Returns true if allowed.
    pub fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Try to consume N tokens.
    pub fn try_consume_n(&mut self, n: u32) -> bool {
        self.refill();
        let needed = n as f64;
        if self.tokens >= needed {
            self.tokens -= needed;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }

    /// Remaining tokens (floored to integer).
    pub fn remaining(&self) -> u32 {
        self.tokens as u32
    }
}

// ---------------------------------------------------------------------------
// Key fingerprints (safe identification)
// ---------------------------------------------------------------------------

/// A key fingerprint for safe identification without exposing material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFingerprint {
    pub key_id: String,
    pub algorithm: String,
    pub chain: Chain,
    /// First 8 bytes of SHA-256(public_key), hex-encoded.
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
}

/// Compute a key fingerprint from a public key.
pub fn compute_fingerprint(public_key: &[u8]) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(public_key);
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn asset_balance_compute_total() {
        let mut ab = AssetBalance::new(Asset::new("ETH"), Chain::Ethereum);
        ab.hot_balance = dec!(10);
        ab.warm_balance = dec!(50);
        ab.cold_balance = dec!(100);
        ab.pending_withdrawals = dec!(5);
        ab.compute_total();
        assert_eq!(ab.total, dec!(160));
        assert_eq!(ab.available, dec!(155));
    }

    #[test]
    fn pnl_calculation() {
        let pnl = CustodyPnl::calculate(dec!(1000), dec!(200), dec!(50), 500);
        assert_eq!(pnl.net_fee_income, dec!(750));
        assert_eq!(pnl.transaction_count, 500);
    }

    #[test]
    fn pnl_negative_when_costs_exceed_income() {
        let pnl = CustodyPnl::calculate(dec!(100), dec!(300), dec!(50), 10);
        assert!(pnl.net_fee_income < Decimal::ZERO);
        assert_eq!(pnl.net_fee_income, dec!(-250));
    }

    #[test]
    fn event_type_count() {
        let events = all_event_types();
        assert_eq!(events.len(), 31);
    }

    #[test]
    fn event_type_categories() {
        assert_eq!(EventType::DepositConfirmed.category(), "deposit");
        assert_eq!(EventType::WithdrawalBroadcast.category(), "withdrawal");
        assert_eq!(EventType::VaultCreated.category(), "vault");
        assert_eq!(EventType::CircuitBreakerTripped.category(), "system");
        assert_eq!(EventType::BackupCreated.category(), "key_management");
    }

    #[test]
    fn event_type_severity() {
        assert_eq!(EventType::SanctionedAddressDetected.severity(), "critical");
        assert_eq!(EventType::WithdrawalFailed.severity(), "warning");
        assert_eq!(EventType::DepositConfirmed.severity(), "info");
    }

    #[test]
    fn event_type_serialization() {
        let json = serde_json::to_string(&EventType::CircuitBreakerTripped).unwrap();
        assert_eq!(json, r#""circuit_breaker_tripped""#);
    }

    #[test]
    fn token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, 1.0);
        assert!(bucket.try_consume());
        assert_eq!(bucket.remaining(), 9);
    }

    #[test]
    fn token_bucket_exhaustion() {
        let mut bucket = TokenBucket::new(2, 0.0); // no refill
        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(!bucket.try_consume()); // exhausted
    }

    #[test]
    fn token_bucket_consume_n() {
        let mut bucket = TokenBucket::new(10, 0.0);
        assert!(bucket.try_consume_n(5));
        assert!(bucket.try_consume_n(5));
        assert!(!bucket.try_consume_n(1));
    }

    #[test]
    fn key_fingerprint_deterministic() {
        let pk = b"test_public_key";
        let fp1 = compute_fingerprint(pk);
        let fp2 = compute_fingerprint(pk);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 16); // 8 bytes = 16 hex chars
    }

    #[test]
    fn key_fingerprint_different_keys() {
        let fp1 = compute_fingerprint(b"key1");
        let fp2 = compute_fingerprint(b"key2");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fee_summary_structure() {
        let summary = FeeSummary {
            period_start: Utc::now(), period_end: Utc::now(),
            chain: Chain::Ethereum,
            total_fees_native: dec!(1.5), total_fees_usd: Some(dec!(4500)),
            transaction_count: 200,
            avg_fee_native: dec!(0.0075),
            max_fee_native: dec!(0.05),
            min_fee_native: dec!(0.001),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("4500"));
    }
}
