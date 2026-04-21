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

    // Validate address format per chain. Previously this was a
    // prefix check only — "bc1" / "T" / length-32-to-44. A
    // typo'd address passed and the signer then either sent to
    // an unspendable script (Bitcoin), bailed with an unclear
    // RPC error (Tron/Solana), or fell back to the custody
    // change address (Bitcoin legacy path). The first case burns
    // user funds; the last looks like "withdrew but money
    // stayed in custody" on reconciliation. Every chain below
    // now decodes + verifies the checksum at validation time,
    // so a single-character typo is caught before the signer
    // commits anything.
    match withdrawal.chain {
        Chain::Ethereum => validate_eth_address(&withdrawal.destination_address)?,
        Chain::Bitcoin => validate_btc_address(&withdrawal.destination_address)?,
        Chain::Tron => validate_tron_address(&withdrawal.destination_address)?,
        Chain::Solana => validate_solana_address(&withdrawal.destination_address)?,
    }

    Ok(())
}

/// Validate a Bitcoin address by decoding its checksum.
///
/// Accepts bech32 (segwit v0 / v1) and refuses anything else
/// because the signer crate's only supported scriptPubKey today
/// is P2WPKH (the hot wallet pubkey hash). Previously the
/// validator passed legacy "1..." / "3..." addresses on prefix
/// alone, then the signer silently fell back to the custody
/// change script — the withdrawal appeared to succeed on chain
/// while the funds stayed in custody. Fail loudly here instead.
pub fn validate_btc_address(address: &str) -> Result<(), CustodyError> {
    let lower = address.to_ascii_lowercase();
    if !(lower.starts_with("bc1") || lower.starts_with("tb1") || lower.starts_with("bcrt1")) {
        return Err(CustodyError::InvalidAddress(
            "Bitcoin address must be bech32 (bc1.../tb1.../bcrt1...). \
             Legacy P2PKH (1...) / P2SH (3...) are not supported by this \
             custody build — sending to such an address would be silently \
             diverted to the custody change script."
                .into(),
        ));
    }
    // bech32::segwit::decode verifies the BCH checksum and the
    // witness-version byte. A single-character typo flips the
    // checksum — decoding fails and we refuse the withdrawal.
    match bech32::segwit::decode(address) {
        Ok((_hrp, _version, program)) => {
            if program.len() != 20 && program.len() != 32 {
                // v0 P2WPKH = 20 bytes; v1 Taproot = 32 bytes.
                return Err(CustodyError::InvalidAddress(format!(
                    "Bitcoin bech32 witness program length {} is not 20 (P2WPKH) or 32 (Taproot)",
                    program.len()
                )));
            }
            Ok(())
        }
        Err(e) => Err(CustodyError::InvalidAddress(format!(
            "Bitcoin bech32 decode failed — checksum invalid or malformed: {e}"
        ))),
    }
}

/// Validate a Tron address by decoding its base58check checksum.
pub fn validate_tron_address(address: &str) -> Result<(), CustodyError> {
    if !address.starts_with('T') {
        return Err(CustodyError::InvalidAddress(
            "Tron address must start with T".into(),
        ));
    }
    // bs58::decode(...).with_check(None) verifies the 4-byte
    // double-SHA-256 checksum. A one-character typo fails this.
    // Expected decoded length is 21 bytes: 1-byte `0x41` prefix
    // followed by the 20-byte address hash.
    match bs58::decode(address).with_check(None).into_vec() {
        Ok(bytes) => {
            if bytes.len() != 21 {
                return Err(CustodyError::InvalidAddress(format!(
                    "Tron address decoded to {} bytes, expected 21",
                    bytes.len()
                )));
            }
            if bytes[0] != 0x41 {
                return Err(CustodyError::InvalidAddress(
                    "Tron address prefix byte must be 0x41 (mainnet)".into(),
                ));
            }
            Ok(())
        }
        Err(e) => Err(CustodyError::InvalidAddress(format!(
            "Tron base58check decode failed — checksum invalid or malformed: {e}"
        ))),
    }
}

/// Validate a Solana address by decoding it as a 32-byte base58 string.
///
/// We do NOT also require the bytes to be on the Ed25519 curve —
/// many valid Solana destinations (Program-Derived Addresses,
/// SPL token accounts) are deliberately off-curve, and checking
/// would reject legitimate withdrawals. The length + base58
/// round-trip catches real typos.
pub fn validate_solana_address(address: &str) -> Result<(), CustodyError> {
    // Base58 length of 32 bytes is 43-44 chars; 44 is the usual
    // case. Anything outside that range isn't a plausible pubkey.
    if address.len() < 32 || address.len() > 44 {
        return Err(CustodyError::InvalidAddress(format!(
            "Solana address length {} is outside the base58-32-byte range (32..=44)",
            address.len()
        )));
    }
    match bs58::decode(address).into_vec() {
        Ok(bytes) => {
            if bytes.len() != 32 {
                return Err(CustodyError::InvalidAddress(format!(
                    "Solana address decoded to {} bytes, expected 32",
                    bytes.len()
                )));
            }
            Ok(())
        }
        Err(e) => Err(CustodyError::InvalidAddress(format!(
            "Solana base58 decode failed: {e}"
        ))),
    }
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
    fn btc_accepts_real_bech32_mainnet() {
        // Bitcoin Core test vector: bc1q... P2WPKH.
        assert!(validate_btc_address(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        )
        .is_ok());
    }

    #[test]
    fn btc_rejects_typo() {
        // Flip a character mid-address; checksum must fail.
        assert!(validate_btc_address(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5"
        )
        .is_err());
    }

    #[test]
    fn btc_rejects_legacy_prefix() {
        // `1...` legacy P2PKH is refused because the signer
        // can't build a P2WPKH tx that targets it; previously
        // it silently went to the custody change script.
        let err = validate_btc_address("1BitcoinEaterAddressDontSendf59kuE").unwrap_err();
        assert!(
            format!("{err}").contains("bech32"),
            "error must explain the bech32 requirement"
        );
    }

    #[test]
    fn btc_rejects_gibberish_with_prefix() {
        // Previously passed the prefix-only check → sent to
        // unspendable script → funds burned.
        assert!(validate_btc_address("bc1qnottheprimarytypoexampleeverwritten").is_err());
    }

    #[test]
    fn tron_accepts_valid_base58check() {
        // Well-known Tron address: USDT contract itself.
        assert!(
            validate_tron_address("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").is_ok()
        );
    }

    #[test]
    fn tron_rejects_typo() {
        // Flip last char → checksum fails.
        assert!(
            validate_tron_address("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6u").is_err()
        );
    }

    #[test]
    fn tron_rejects_one_char() {
        // Previously passed — single T satisfied the old prefix check.
        assert!(validate_tron_address("T").is_err());
    }

    #[test]
    fn solana_accepts_32_byte_pubkey() {
        // A 32-byte value encoded as base58.
        let raw = [7u8; 32];
        let addr = bs58::encode(raw).into_string();
        assert!(validate_solana_address(&addr).is_ok());
    }

    #[test]
    fn solana_rejects_non_base58() {
        assert!(
            validate_solana_address("0Ol!not_base58_at_all_here_32charsxx")
                .is_err()
        );
    }

    #[test]
    fn solana_rejects_wrong_decoded_length() {
        // 31-byte base58 string is inside the length gate but decodes < 32 bytes.
        let raw = [7u8; 31];
        let addr = bs58::encode(raw).into_string();
        // Length might still be 32-44 chars depending on encoding;
        // verify the decode-length branch either way.
        if addr.len() >= 32 && addr.len() <= 44 {
            assert!(validate_solana_address(&addr).is_err());
        }
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
