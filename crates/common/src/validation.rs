//! Multi-chain address validation with real regex patterns.
//!
//! Every address MUST be validated before accepting deposits or processing withdrawals.

use crate::Chain;

/// Validate a blockchain address format for a given chain.
///
/// Returns `Ok(())` if valid, `Err(reason)` if not.
pub fn validate_address(chain: Chain, address: &str) -> Result<(), String> {
    match chain {
        Chain::Bitcoin => validate_bitcoin_address(address),
        Chain::Ethereum => validate_ethereum_address(address),
        Chain::Tron => validate_tron_address(address),
        Chain::Solana => validate_solana_address(address),
    }
}

/// Bitcoin: Bech32 (bc1q/bc1p), Legacy (1), P2SH (3)
fn validate_bitcoin_address(address: &str) -> Result<(), String> {
    if address.is_empty() {
        return Err("Address is empty".into());
    }

    // Bech32 native segwit (P2WPKH or P2WSH)
    if address.starts_with("bc1q") {
        if address.len() != 42 && address.len() != 62 {
            return Err(format!("Bech32 address wrong length: {} (expected 42 or 62)", address.len()));
        }
        // Bech32 characters: qpzry9x8gf2tvdw0s3jn54khce6mua7l
        let valid_chars = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
        for c in address[4..].chars() {
            if !valid_chars.contains(c) {
                return Err(format!("Invalid Bech32 character: {c}"));
            }
        }
        return Ok(());
    }

    // Bech32m taproot (P2TR)
    if address.starts_with("bc1p") {
        if address.len() != 62 {
            return Err(format!("Taproot address wrong length: {} (expected 62)", address.len()));
        }
        return Ok(());
    }

    // Legacy P2PKH (1...)
    if address.starts_with('1') {
        if address.len() < 25 || address.len() > 34 {
            return Err("Legacy address wrong length".into());
        }
        return validate_base58(address);
    }

    // P2SH (3...)
    if address.starts_with('3') {
        if address.len() < 25 || address.len() > 34 {
            return Err("P2SH address wrong length".into());
        }
        return validate_base58(address);
    }

    // Testnet
    if address.starts_with("tb1") || address.starts_with('m') || address.starts_with('n') || address.starts_with('2') {
        return Ok(()); // testnet addresses
    }

    Err(format!("Unrecognized Bitcoin address format: {}", &address[..address.len().min(10)]))
}

/// Ethereum: 0x-prefixed hex, 42 chars, optional EIP-55 checksum.
fn validate_ethereum_address(address: &str) -> Result<(), String> {
    if !address.starts_with("0x") && !address.starts_with("0X") {
        return Err("Ethereum address must start with 0x".into());
    }
    if address.len() != 42 {
        return Err(format!("Ethereum address wrong length: {} (expected 42)", address.len()));
    }
    // Validate hex chars
    for c in address[2..].chars() {
        if !c.is_ascii_hexdigit() {
            return Err(format!("Invalid hex character in address: {c}"));
        }
    }
    // Zero address check
    if address == "0x0000000000000000000000000000000000000000" {
        return Err("Zero address not allowed".into());
    }
    Ok(())
}

/// Tron: Base58Check, starts with T, 34 chars.
fn validate_tron_address(address: &str) -> Result<(), String> {
    if !address.starts_with('T') {
        return Err("Tron address must start with T".into());
    }
    if address.len() != 34 {
        return Err(format!("Tron address wrong length: {} (expected 34)", address.len()));
    }
    validate_base58(address)
}

/// Solana: Base58, 32-44 chars.
fn validate_solana_address(address: &str) -> Result<(), String> {
    if address.len() < 32 || address.len() > 44 {
        return Err(format!("Solana address wrong length: {} (expected 32-44)", address.len()));
    }
    validate_base58(address)
}

/// Validate Base58 characters (no 0, O, I, l).
fn validate_base58(address: &str) -> Result<(), String> {
    const BASE58_CHARS: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    for c in address.chars() {
        if !BASE58_CHARS.contains(c) {
            return Err(format!("Invalid Base58 character: {c}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Error catalog
// ---------------------------------------------------------------------------

/// Standardized error codes for the custody API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // General (1xxx)
    InternalError = 1000,
    BadRequest = 1001,
    NotFound = 1002,
    Conflict = 1003,
    Unauthorized = 1004,
    RateLimited = 1005,
    MaintenanceMode = 1006,

    // Validation (2xxx)
    InvalidAddress = 2001,
    InvalidAmount = 2002,
    InvalidChain = 2003,
    InvalidAsset = 2004,
    InsufficientBalance = 2005,
    AmountBelowMinimum = 2006,
    AmountAboveMaximum = 2007,

    // Vault (3xxx)
    VaultNotFound = 3001,
    VaultFrozen = 3002,
    VaultHasBalance = 3003,
    AccountNotFound = 3004,
    AccountFrozen = 3005,

    // Transaction (4xxx)
    WithdrawalNotFound = 4001,
    WithdrawalAlreadyExists = 4002,
    InvalidWithdrawalStatus = 4003,
    TransactionNotCancellable = 4004,
    CircuitBreakerTripped = 4005,
    SanctionedAddress = 4006,

    // Signing (5xxx)
    SigningFailed = 5001,
    KeyNotFound = 5002,
    UnsupportedChain = 5003,
    InvalidEncoding = 5004,

    // Policy (6xxx)
    PolicyViolation = 6001,
    ApprovalRequired = 6002,
    ApprovalExpired = 6003,
    DuplicateApprover = 6004,

    // Compliance (7xxx)
    ScreeningFailed = 7001,
    TravelRuleRequired = 7002,
    HighRiskAddress = 7003,
}

impl ErrorCode {
    pub fn code(&self) -> u32 {
        *self as u32
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InternalError => "Internal server error",
            Self::BadRequest => "Bad request",
            Self::NotFound => "Resource not found",
            Self::Conflict => "Resource conflict",
            Self::Unauthorized => "Unauthorized",
            Self::RateLimited => "Rate limit exceeded",
            Self::MaintenanceMode => "Service in maintenance mode",
            Self::InvalidAddress => "Invalid blockchain address",
            Self::InvalidAmount => "Invalid amount",
            Self::InvalidChain => "Invalid or unsupported chain",
            Self::InvalidAsset => "Invalid or unsupported asset",
            Self::InsufficientBalance => "Insufficient balance",
            Self::AmountBelowMinimum => "Amount below minimum",
            Self::AmountAboveMaximum => "Amount above maximum",
            Self::VaultNotFound => "Vault not found",
            Self::VaultFrozen => "Vault is frozen",
            Self::VaultHasBalance => "Vault has non-zero balance",
            Self::AccountNotFound => "Vault account not found",
            Self::AccountFrozen => "Vault account is frozen",
            Self::WithdrawalNotFound => "Withdrawal not found",
            Self::WithdrawalAlreadyExists => "Withdrawal already exists (idempotency)",
            Self::InvalidWithdrawalStatus => "Withdrawal in invalid status for this operation",
            Self::TransactionNotCancellable => "Transaction cannot be cancelled",
            Self::CircuitBreakerTripped => "Circuit breaker limit exceeded",
            Self::SanctionedAddress => "Address is on sanctions list",
            Self::SigningFailed => "Transaction signing failed",
            Self::KeyNotFound => "Signing key not found",
            Self::UnsupportedChain => "Chain not supported for this operation",
            Self::InvalidEncoding => "Invalid message encoding",
            Self::PolicyViolation => "Vault policy violation",
            Self::ApprovalRequired => "Approval required for this operation",
            Self::ApprovalExpired => "Approval has expired",
            Self::DuplicateApprover => "Approver has already submitted a decision",
            Self::ScreeningFailed => "Compliance screening failed",
            Self::TravelRuleRequired => "Travel rule data required",
            Self::HighRiskAddress => "Address flagged as high risk",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

// ---------------------------------------------------------------------------
// Memo / destination-tag validation
// ---------------------------------------------------------------------------

/// Chain-specific rules for `memo` and `destination_tag` fields on a
/// withdrawal. Accepting silent junk here is a footgun: a Solana
/// user attaching a memo expecting it to route to an exchange account
/// will watch their funds vanish because the SOL signer path doesn't
/// attach it. Reject at the edge instead of hoping signing catches it.
///
/// Rules:
/// - **Bitcoin / Ethereum / Tron**: no protocol-level memo or tag
///   carried with a plain value transfer — reject if either is set.
/// - **Solana**: an SPL Memo instruction can carry up to 566 bytes of
///   UTF-8. We allow `memo` but cap at 512 to stay well under the
///   practical limit once fees/signatures are added. `destination_tag`
///   is always rejected — SOL has no numeric tag concept.
///
/// When `Chain` grows Cosmos / XRP / Stellar variants, this is the
/// one place that needs updating — the handler call site doesn't
/// change.
pub fn validate_withdrawal_memo(
    chain: Chain,
    memo: Option<&str>,
    destination_tag: Option<u64>,
) -> Result<(), String> {
    match chain {
        Chain::Bitcoin => {
            if memo.is_some() {
                return Err("Bitcoin does not carry memos on transfers".into());
            }
            if destination_tag.is_some() {
                return Err("Bitcoin does not use destination tags".into());
            }
        }
        Chain::Ethereum => {
            if memo.is_some() {
                return Err(
                    "Ethereum transfers do not carry memos. Use a contract call with a data field if routing is needed".into(),
                );
            }
            if destination_tag.is_some() {
                return Err("Ethereum does not use destination tags".into());
            }
        }
        Chain::Tron => {
            if memo.is_some() {
                return Err("Tron transfers do not carry memos".into());
            }
            if destination_tag.is_some() {
                return Err("Tron does not use destination tags".into());
            }
        }
        Chain::Solana => {
            if destination_tag.is_some() {
                return Err(
                    "Solana does not use numeric destination tags. Attach an SPL memo instead".into(),
                );
            }
            if let Some(m) = memo {
                // The SPL Memo program accepts up to ~566 bytes but
                // practical transactions must also fit fees, account
                // keys, and the transfer instruction inside the 1232-
                // byte packet cap. 512 is a safe ceiling.
                const MAX: usize = 512;
                if m.is_empty() {
                    return Err("Solana memo is present but empty — omit the field instead".into());
                }
                if m.as_bytes().len() > MAX {
                    return Err(format!(
                        "Solana memo exceeds {MAX} bytes (got {})",
                        m.as_bytes().len()
                    ));
                }
                // `m` is already guaranteed UTF-8 by virtue of being `&str`;
                // explicit NUL check catches someone splicing a raw byte.
                if m.contains('\0') {
                    return Err("Solana memo must not contain NUL bytes".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Bitcoin ---

    #[test]
    fn valid_bitcoin_bech32() {
        assert!(validate_address(Chain::Bitcoin, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu").is_ok());
    }

    #[test]
    fn valid_bitcoin_legacy() {
        assert!(validate_address(Chain::Bitcoin, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_ok());
    }

    #[test]
    fn valid_bitcoin_p2sh() {
        assert!(validate_address(Chain::Bitcoin, "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy").is_ok());
    }

    #[test]
    fn invalid_bitcoin_too_short() {
        assert!(validate_address(Chain::Bitcoin, "bc1q").is_err());
    }

    #[test]
    fn invalid_bitcoin_bad_prefix() {
        assert!(validate_address(Chain::Bitcoin, "xc1qtest").is_err());
    }

    // --- Ethereum ---

    #[test]
    fn valid_ethereum_address() {
        assert!(validate_address(Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18").is_ok());
    }

    #[test]
    fn valid_ethereum_lowercase() {
        assert!(validate_address(Chain::Ethereum, "0xde0b295669a9fd93d5f28d9ec85e40f4cb697bae").is_ok());
    }

    #[test]
    fn invalid_ethereum_no_prefix() {
        assert!(validate_address(Chain::Ethereum, "742d35Cc6634C0532925a3b844Bc9e7595f2bD18").is_err());
    }

    #[test]
    fn invalid_ethereum_wrong_length() {
        assert!(validate_address(Chain::Ethereum, "0x742d35Cc").is_err());
    }

    #[test]
    fn invalid_ethereum_zero_address() {
        assert!(validate_address(Chain::Ethereum, "0x0000000000000000000000000000000000000000").is_err());
    }

    #[test]
    fn invalid_ethereum_bad_hex() {
        assert!(validate_address(Chain::Ethereum, "0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG").is_err());
    }

    // --- Tron ---

    #[test]
    fn valid_tron_address() {
        assert!(validate_address(Chain::Tron, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").is_ok());
    }

    #[test]
    fn invalid_tron_no_t_prefix() {
        assert!(validate_address(Chain::Tron, "AR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").is_err());
    }

    #[test]
    fn invalid_tron_wrong_length() {
        assert!(validate_address(Chain::Tron, "Tabc").is_err());
    }

    // --- Solana ---

    #[test]
    fn valid_solana_address() {
        assert!(validate_address(Chain::Solana, "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").is_ok());
    }

    #[test]
    fn invalid_solana_too_short() {
        assert!(validate_address(Chain::Solana, "abc").is_err());
    }

    #[test]
    fn invalid_solana_bad_chars() {
        assert!(validate_address(Chain::Solana, "0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl").is_err());
    }

    // --- Error catalog ---

    #[test]
    fn error_codes_unique() {
        let codes = vec![
            ErrorCode::InternalError, ErrorCode::BadRequest, ErrorCode::NotFound,
            ErrorCode::InvalidAddress, ErrorCode::InsufficientBalance,
            ErrorCode::VaultNotFound, ErrorCode::VaultFrozen,
            ErrorCode::WithdrawalNotFound, ErrorCode::CircuitBreakerTripped,
            ErrorCode::SigningFailed, ErrorCode::PolicyViolation,
            ErrorCode::ScreeningFailed, ErrorCode::TravelRuleRequired,
        ];
        let mut seen = std::collections::HashSet::new();
        for code in &codes {
            assert!(seen.insert(code.code()), "Duplicate error code: {}", code.code());
        }
    }

    #[test]
    fn error_code_display() {
        let err = ErrorCode::InvalidAddress;
        let s = err.to_string();
        assert!(s.contains("2001"));
        assert!(s.contains("Invalid blockchain address"));
    }

    #[test]
    fn error_code_ranges() {
        assert!(ErrorCode::InternalError.code() >= 1000 && ErrorCode::InternalError.code() < 2000);
        assert!(ErrorCode::InvalidAddress.code() >= 2000 && ErrorCode::InvalidAddress.code() < 3000);
        assert!(ErrorCode::VaultNotFound.code() >= 3000 && ErrorCode::VaultNotFound.code() < 4000);
        assert!(ErrorCode::WithdrawalNotFound.code() >= 4000 && ErrorCode::WithdrawalNotFound.code() < 5000);
        assert!(ErrorCode::SigningFailed.code() >= 5000 && ErrorCode::SigningFailed.code() < 6000);
        assert!(ErrorCode::PolicyViolation.code() >= 6000 && ErrorCode::PolicyViolation.code() < 7000);
        assert!(ErrorCode::ScreeningFailed.code() >= 7000 && ErrorCode::ScreeningFailed.code() < 8000);
    }

    // --- Memo / destination-tag ---

    #[test]
    fn memo_none_is_accepted_on_all_chains() {
        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            assert!(validate_withdrawal_memo(chain, None, None).is_ok());
        }
    }

    #[test]
    fn memo_rejected_on_btc_eth_tron() {
        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron] {
            let err = validate_withdrawal_memo(chain, Some("anything"), None).unwrap_err();
            assert!(err.to_lowercase().contains("memo") || err.to_lowercase().contains("transfers"));
        }
    }

    #[test]
    fn destination_tag_rejected_on_all_current_chains() {
        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            let err = validate_withdrawal_memo(chain, None, Some(42)).unwrap_err();
            assert!(err.to_lowercase().contains("tag") || err.to_lowercase().contains("numeric"));
        }
    }

    #[test]
    fn solana_memo_within_limit_ok() {
        let m: String = "a".repeat(100);
        assert!(validate_withdrawal_memo(Chain::Solana, Some(&m), None).is_ok());
    }

    #[test]
    fn solana_memo_too_long_rejected() {
        let m: String = "a".repeat(513);
        let err = validate_withdrawal_memo(Chain::Solana, Some(&m), None).unwrap_err();
        assert!(err.contains("exceeds") && err.contains("512"));
    }

    #[test]
    fn solana_memo_empty_string_rejected() {
        let err = validate_withdrawal_memo(Chain::Solana, Some(""), None).unwrap_err();
        assert!(err.to_lowercase().contains("empty"));
    }

    #[test]
    fn solana_memo_nul_byte_rejected() {
        let err = validate_withdrawal_memo(Chain::Solana, Some("hello\0world"), None).unwrap_err();
        assert!(err.contains("NUL"));
    }
}
