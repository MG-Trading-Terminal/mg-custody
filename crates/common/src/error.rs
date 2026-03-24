//! Custody service error types.

use thiserror::Error;

/// Top-level custody service error.
#[derive(Debug, Error)]
pub enum CustodyError {
    // -- Address errors --
    #[error("address already exists for user {user_id}, asset {asset}, chain {chain}")]
    AddressAlreadyExists {
        user_id: uuid::Uuid,
        asset: String,
        chain: String,
    },

    #[error("invalid address format: {0}")]
    InvalidAddress(String),

    // -- Withdrawal errors --
    #[error("duplicate withdrawal: exchange_id {0} already submitted")]
    DuplicateWithdrawal(uuid::Uuid),

    #[error("withdrawal {0} not found")]
    WithdrawalNotFound(uuid::Uuid),

    #[error("insufficient hot wallet balance: need {needed}, have {available}")]
    InsufficientBalance {
        needed: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    #[error("withdrawal blocked by circuit breaker: {reason}")]
    CircuitBreakerTripped { reason: String },

    #[error("withdrawal requires approval (amount exceeds threshold)")]
    ApprovalRequired,

    #[error("withdrawal cancelled")]
    WithdrawalCancelled,

    #[error("withdrawal in invalid state {current} for operation {operation}")]
    InvalidState { current: String, operation: String },

    // -- Signing errors --
    #[error("signing failed: {0}")]
    SigningError(String),

    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    // -- Chain / node errors --
    #[error("chain {chain} node error: {message}")]
    NodeError { chain: String, message: String },

    #[error("transaction broadcast failed: {0}")]
    BroadcastError(String),

    #[error("transaction not found: {0}")]
    TransactionNotFound(String),

    // -- Compliance --
    #[error("address {0} blocked by sanctions screening")]
    SanctionedAddress(String),

    // -- Infrastructure --
    #[error("database error: {0}")]
    Database(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Result alias for custody operations.
pub type CustodyResult<T> = Result<T, CustodyError>;
