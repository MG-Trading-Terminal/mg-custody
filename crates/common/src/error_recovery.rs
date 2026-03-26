//! Error recovery patterns — what to do when things go wrong.
//!
//! Every chain operation can fail. This module defines recovery strategies
//! for each failure mode to prevent fund loss.

use serde::{Deserialize, Serialize};
use crate::Chain;

/// Recovery action to take for a failed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    /// Retry the same operation (transient failure).
    Retry,
    /// Wait and retry with exponential backoff.
    RetryWithBackoff,
    /// Replace the transaction (RBF/nonce replacement).
    ReplaceTransaction,
    /// Abort and mark as failed (permanent failure).
    Abort,
    /// Escalate to manual review.
    ManualReview,
    /// Rebuild with fresh parameters (new blockhash, new nonce).
    Rebuild,
    /// Skip and continue (non-critical).
    Skip,
}

/// Classify an RPC error and determine recovery.
pub fn classify_rpc_error(_chain: Chain, error_msg: &str) -> (ErrorClass, RecoveryAction) {
    let lower = error_msg.to_lowercase();

    // Connection errors — always retry
    if lower.contains("connection refused") || lower.contains("timeout") || lower.contains("eof") {
        return (ErrorClass::ConnectionError, RecoveryAction::RetryWithBackoff);
    }

    // Rate limiting — backoff
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many") {
        return (ErrorClass::RateLimited, RecoveryAction::RetryWithBackoff);
    }

    // Nonce errors (Ethereum)
    if lower.contains("nonce too low") {
        return (ErrorClass::NonceTooLow, RecoveryAction::Rebuild);
    }
    if lower.contains("nonce too high") || lower.contains("replacement transaction underpriced") {
        return (ErrorClass::NonceTooHigh, RecoveryAction::RetryWithBackoff);
    }

    // Gas/fee errors
    if lower.contains("insufficient funds") || lower.contains("gas too low") {
        return (ErrorClass::InsufficientFunds, RecoveryAction::ManualReview);
    }
    if lower.contains("max fee per gas") || lower.contains("gas price") {
        return (ErrorClass::GasPriceTooLow, RecoveryAction::Rebuild);
    }

    // Transaction already known
    if lower.contains("already known") || lower.contains("already imported") {
        return (ErrorClass::AlreadyKnown, RecoveryAction::Skip);
    }

    // Solana-specific
    if lower.contains("blockhash not found") || lower.contains("blockhash expired") {
        return (ErrorClass::BlockhashExpired, RecoveryAction::Rebuild);
    }

    // Tron-specific
    if lower.contains("energy") || lower.contains("bandwidth") {
        return (ErrorClass::InsufficientResources, RecoveryAction::ManualReview);
    }

    // Bitcoin-specific
    if lower.contains("mempool") || lower.contains("too-long-mempool-chain") {
        return (ErrorClass::MempoolFull, RecoveryAction::RetryWithBackoff);
    }

    // Unknown error
    (ErrorClass::Unknown, RecoveryAction::ManualReview)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    ConnectionError,
    RateLimited,
    NonceTooLow,
    NonceTooHigh,
    InsufficientFunds,
    GasPriceTooLow,
    AlreadyKnown,
    BlockhashExpired,
    InsufficientResources,
    MempoolFull,
    Unknown,
}

/// Recovery state for a stuck transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckTransaction {
    pub tx_hash: String,
    pub chain: Chain,
    pub age_seconds: u64,
    pub stuck_reason: StuckReason,
    pub recommended_action: RecoveryAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StuckReason {
    LowFee,
    NoncGap,
    MempoolDrop,
    NodeOutOfSync,
    Unknown,
}

/// Determine if a transaction is stuck based on age and chain.
pub fn is_stuck(chain: Chain, age_seconds: u64) -> bool {
    let threshold = match chain {
        Chain::Ethereum => 300,   // 5 min (25 blocks)
        Chain::Bitcoin => 3600,   // 1 hour (6 blocks)
        Chain::Tron => 60,        // 1 min (20 blocks)
        Chain::Solana => 90,      // 90 sec (blockhash TTL)
    };
    age_seconds > threshold
}

/// Determine recovery for a stuck transaction.
pub fn recommend_stuck_recovery(chain: Chain, age_seconds: u64) -> RecoveryAction {
    match chain {
        Chain::Ethereum => {
            if age_seconds > 3600 { RecoveryAction::ReplaceTransaction }
            else { RecoveryAction::RetryWithBackoff }
        }
        Chain::Bitcoin => {
            if age_seconds > 7200 { RecoveryAction::ReplaceTransaction } // RBF
            else { RecoveryAction::RetryWithBackoff }
        }
        Chain::Solana => {
            // Solana txs expire after ~90 sec, must rebuild
            RecoveryAction::Rebuild
        }
        Chain::Tron => {
            if age_seconds > 300 { RecoveryAction::Rebuild }
            else { RecoveryAction::Retry }
        }
    }
}

/// Maximum retry attempts per chain.
pub fn max_retries(chain: Chain) -> u32 {
    match chain {
        Chain::Ethereum => 5,
        Chain::Bitcoin => 3,
        Chain::Tron => 5,
        Chain::Solana => 10, // fast retries, txs expire quickly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_connection_timeout() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "connection timeout after 30s");
        assert_eq!(class, ErrorClass::ConnectionError);
        assert_eq!(action, RecoveryAction::RetryWithBackoff);
    }

    #[test]
    fn classify_rate_limit() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "HTTP 429 Too Many Requests");
        assert_eq!(class, ErrorClass::RateLimited);
        assert_eq!(action, RecoveryAction::RetryWithBackoff);
    }

    #[test]
    fn classify_nonce_too_low() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "nonce too low: expected 15, got 10");
        assert_eq!(class, ErrorClass::NonceTooLow);
        assert_eq!(action, RecoveryAction::Rebuild);
    }

    #[test]
    fn classify_insufficient_funds() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "insufficient funds for gas * price + value");
        assert_eq!(class, ErrorClass::InsufficientFunds);
        assert_eq!(action, RecoveryAction::ManualReview);
    }

    #[test]
    fn classify_already_known() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "already known");
        assert_eq!(class, ErrorClass::AlreadyKnown);
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn classify_blockhash_expired() {
        let (class, action) = classify_rpc_error(Chain::Solana, "blockhash not found");
        assert_eq!(class, ErrorClass::BlockhashExpired);
        assert_eq!(action, RecoveryAction::Rebuild);
    }

    #[test]
    fn classify_tron_energy() {
        let (class, action) = classify_rpc_error(Chain::Tron, "Not enough energy");
        assert_eq!(class, ErrorClass::InsufficientResources);
        assert_eq!(action, RecoveryAction::ManualReview);
    }

    #[test]
    fn classify_unknown() {
        let (class, action) = classify_rpc_error(Chain::Ethereum, "some weird error");
        assert_eq!(class, ErrorClass::Unknown);
        assert_eq!(action, RecoveryAction::ManualReview);
    }

    #[test]
    fn stuck_detection_eth() {
        assert!(!is_stuck(Chain::Ethereum, 100));
        assert!(is_stuck(Chain::Ethereum, 600));
    }

    #[test]
    fn stuck_detection_sol() {
        assert!(!is_stuck(Chain::Solana, 30));
        assert!(is_stuck(Chain::Solana, 120));
    }

    #[test]
    fn stuck_recovery_sol_always_rebuild() {
        assert_eq!(recommend_stuck_recovery(Chain::Solana, 100), RecoveryAction::Rebuild);
    }

    #[test]
    fn stuck_recovery_eth_replace_after_hour() {
        assert_eq!(recommend_stuck_recovery(Chain::Ethereum, 7200), RecoveryAction::ReplaceTransaction);
        assert_eq!(recommend_stuck_recovery(Chain::Ethereum, 600), RecoveryAction::RetryWithBackoff);
    }

    #[test]
    fn max_retries_solana_highest() {
        assert!(max_retries(Chain::Solana) > max_retries(Chain::Bitcoin));
    }

    #[test]
    fn recovery_action_serialization() {
        assert_eq!(serde_json::to_string(&RecoveryAction::Rebuild).unwrap(), r#""rebuild""#);
        assert_eq!(serde_json::to_string(&RecoveryAction::ManualReview).unwrap(), r#""manual_review""#);
    }

    #[test]
    fn error_class_serialization() {
        assert_eq!(serde_json::to_string(&ErrorClass::BlockhashExpired).unwrap(), r#""blockhash_expired""#);
    }

    #[test]
    fn stuck_reason_serialization() {
        assert_eq!(serde_json::to_string(&StuckReason::LowFee).unwrap(), r#""low_fee""#);
    }
}
