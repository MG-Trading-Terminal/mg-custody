//! Withdrawal request validation.
//!
//! Validates address format, amount, and supported assets before
//! a withdrawal enters the signing pipeline.

use anyhow::Result;
use custody_common::{Chain, CustodyError, Withdrawal};
use rust_decimal::Decimal;

/// Validate an Ethereum address format.
pub fn validate_eth_address(address: &str) -> Result<(), CustodyError> {
    if !address.starts_with("0x") {
        return Err(CustodyError::InvalidAddress(
            "Ethereum address must start with 0x".into(),
        ));
    }
    if address.len() != 42 {
        return Err(CustodyError::InvalidAddress(format!(
            "Ethereum address must be 42 characters, got {}",
            address.len()
        )));
    }
    // Check all remaining chars are hex
    if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CustodyError::InvalidAddress(
            "Ethereum address contains non-hex characters".into(),
        ));
    }
    Ok(())
}

/// Validate a withdrawal request.
pub fn validate_withdrawal(withdrawal: &Withdrawal) -> Result<(), CustodyError> {
    // Amount must be positive
    if withdrawal.amount <= Decimal::ZERO {
        return Err(CustodyError::InvalidState {
            current: "Received".into(),
            operation: "Amount must be positive".into(),
        });
    }

    // Validate address format per chain
    match withdrawal.chain {
        Chain::Ethereum => validate_eth_address(&withdrawal.destination_address)?,
        Chain::Bitcoin => {
            if !withdrawal.destination_address.starts_with("bc1")
                && !withdrawal.destination_address.starts_with("tb1")
                && !withdrawal.destination_address.starts_with('1')
                && !withdrawal.destination_address.starts_with('3')
            {
                return Err(CustodyError::InvalidAddress(
                    "Invalid Bitcoin address prefix".into(),
                ));
            }
        }
        Chain::Tron => {
            if !withdrawal.destination_address.starts_with('T') {
                return Err(CustodyError::InvalidAddress(
                    "Tron address must start with T".into(),
                ));
            }
        }
        Chain::Solana => {
            if withdrawal.destination_address.len() < 32
                || withdrawal.destination_address.len() > 44
            {
                return Err(CustodyError::InvalidAddress(
                    "Invalid Solana address length".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Check circuit breaker limits.
///
/// Returns an error if the withdrawal would exceed any limit.
pub fn check_circuit_breaker(
    amount_usd: Decimal,
    max_single: Decimal,
    hourly_used: Decimal,
    max_hourly: Decimal,
    daily_used: Decimal,
    max_daily: Decimal,
) -> Result<(), CustodyError> {
    if amount_usd > max_single {
        return Err(CustodyError::CircuitBreakerTripped {
            reason: format!(
                "Single withdrawal ${amount_usd} exceeds limit ${max_single}"
            ),
        });
    }
    if hourly_used + amount_usd > max_hourly {
        return Err(CustodyError::CircuitBreakerTripped {
            reason: format!(
                "Hourly outflow ${} would exceed limit ${max_hourly}",
                hourly_used + amount_usd
            ),
        });
    }
    if daily_used + amount_usd > max_daily {
        return Err(CustodyError::CircuitBreakerTripped {
            reason: format!(
                "Daily outflow ${} would exceed limit ${max_daily}",
                daily_used + amount_usd
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn valid_eth_address() {
        assert!(validate_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18").is_ok());
        assert!(validate_eth_address("0x0000000000000000000000000000000000000000").is_ok());
    }

    #[test]
    fn invalid_eth_address_no_prefix() {
        assert!(validate_eth_address("742d35Cc6634C0532925a3b844Bc9e7595f2bD18").is_err());
    }

    #[test]
    fn invalid_eth_address_wrong_length() {
        assert!(validate_eth_address("0x742d").is_err());
        assert!(validate_eth_address("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18FF").is_err());
    }

    #[test]
    fn invalid_eth_address_non_hex() {
        assert!(validate_eth_address("0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_err());
    }

    #[test]
    fn circuit_breaker_within_limits() {
        assert!(check_circuit_breaker(
            dec!(1000),
            dec!(500000),
            dec!(0),
            dec!(10000000),
            dec!(0),
            dec!(50000000),
        )
        .is_ok());
    }

    #[test]
    fn circuit_breaker_single_limit() {
        let result = check_circuit_breaker(
            dec!(600000),   // exceeds 500K single limit
            dec!(500000),
            dec!(0),
            dec!(10000000),
            dec!(0),
            dec!(50000000),
        );
        assert!(matches!(result, Err(CustodyError::CircuitBreakerTripped { .. })));
    }

    #[test]
    fn circuit_breaker_hourly_limit() {
        let result = check_circuit_breaker(
            dec!(1000000),
            dec!(500000000), // single OK
            dec!(9500000),   // hourly already 9.5M
            dec!(10000000),  // hourly limit 10M → 9.5M + 1M = 10.5M > 10M
            dec!(0),
            dec!(50000000),
        );
        assert!(matches!(result, Err(CustodyError::CircuitBreakerTripped { .. })));
    }

    #[test]
    fn circuit_breaker_daily_limit() {
        let result = check_circuit_breaker(
            dec!(1000000),
            dec!(500000000),
            dec!(0),
            dec!(100000000),
            dec!(49500000),  // daily already 49.5M
            dec!(50000000),  // daily limit 50M → 49.5M + 1M = 50.5M > 50M
        );
        assert!(matches!(result, Err(CustodyError::CircuitBreakerTripped { .. })));
    }
}
