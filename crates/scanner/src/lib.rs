//! Blockchain deposit scanners.
//!
//! One scanner per chain. Each polls new blocks and checks transactions
//! against our known deposit address set.

pub mod address_cache;
pub mod bitcoin;
pub mod ethereum;
pub mod solana;
pub mod tron;

use custody_common::{Chain, Deposit};

/// Trait for chain-specific block scanners.
pub trait ChainScanner: Send + Sync {
    /// Which chain this scanner handles.
    fn chain(&self) -> Chain;

    /// Scan blocks from `from_block` to the current chain tip.
    /// Returns newly detected deposits and the last scanned block height.
    fn scan(
        &self,
        from_block: u64,
    ) -> impl std::future::Future<Output = anyhow::Result<(Vec<Deposit>, u64)>> + Send;

    /// Get the current block height from the chain node.
    fn current_block_height(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<u64>> + Send;
}

/// Convert a uint256 token amount to a Decimal, given the token's decimal places.
///
/// This is the ONLY place token amount conversion should happen.
/// Using this function consistently prevents decimal/precision bugs.
///
/// Examples:
///   - 1_000_000_000_000_000_000 with 18 decimals → Decimal(1.0) (1 ETH)
///   - 1_000_000 with 6 decimals → Decimal(1.0) (1 USDT)
pub fn uint256_to_decimal(raw: alloy::primitives::U256, decimals: u8) -> rust_decimal::Decimal {
    // U256 can exceed Decimal's range, so we convert via string
    let raw_str = raw.to_string();
    let raw_decimal = rust_decimal::Decimal::from_str_exact(&raw_str).unwrap_or(rust_decimal::Decimal::MAX);
    let divisor = rust_decimal::Decimal::new(10i64.pow(decimals as u32), 0);
    raw_decimal / divisor
}

/// Convert a Decimal amount to U256 base units.
///
/// Inverse of `uint256_to_decimal`.
pub fn decimal_to_uint256(amount: rust_decimal::Decimal, decimals: u8) -> alloy::primitives::U256 {
    let multiplier = rust_decimal::Decimal::new(10i64.pow(decimals as u32), 0);
    let base_units = (amount * multiplier).trunc();
    use rust_decimal::prelude::ToPrimitive;
    let value = base_units.to_u128().unwrap_or(0);
    alloy::primitives::U256::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::U256;
    use rust_decimal_macros::dec;

    #[test]
    fn uint256_to_decimal_eth_18() {
        // 1 ETH = 10^18 wei
        let raw = U256::from(1_000_000_000_000_000_000u128);
        assert_eq!(uint256_to_decimal(raw, 18), dec!(1));
    }

    #[test]
    fn uint256_to_decimal_usdt_6() {
        // 1 USDT = 10^6 base units
        let raw = U256::from(1_000_000u128);
        assert_eq!(uint256_to_decimal(raw, 6), dec!(1));
    }

    #[test]
    fn uint256_to_decimal_usdc_6() {
        // 1 USDC = 10^6 base units
        let raw = U256::from(1_000_000u128);
        assert_eq!(uint256_to_decimal(raw, 6), dec!(1));
    }

    #[test]
    fn uint256_to_decimal_fractional_eth() {
        // 0.5 ETH = 5 * 10^17 wei
        let raw = U256::from(500_000_000_000_000_000u128);
        assert_eq!(uint256_to_decimal(raw, 18), dec!(0.5));
    }

    #[test]
    fn uint256_to_decimal_large_usdt() {
        // 1,000,000 USDT = 10^12 base units
        let raw = U256::from(1_000_000_000_000u128);
        assert_eq!(uint256_to_decimal(raw, 6), dec!(1000000));
    }

    #[test]
    fn uint256_to_decimal_zero() {
        assert_eq!(uint256_to_decimal(U256::ZERO, 18), dec!(0));
        assert_eq!(uint256_to_decimal(U256::ZERO, 6), dec!(0));
    }

    #[test]
    fn uint256_to_decimal_small_wei() {
        // 1 wei = 10^-18 ETH
        let raw = U256::from(1u128);
        let result = uint256_to_decimal(raw, 18);
        assert!(result > dec!(0));
        assert!(result < dec!(0.000000000000000001) || result == dec!(0.000000000000000001));
    }

    #[test]
    fn decimal_to_uint256_eth() {
        let amount = dec!(1.5);
        let result = decimal_to_uint256(amount, 18);
        assert_eq!(result, U256::from(1_500_000_000_000_000_000u128));
    }

    #[test]
    fn decimal_to_uint256_usdt() {
        let amount = dec!(100.50);
        let result = decimal_to_uint256(amount, 6);
        assert_eq!(result, U256::from(100_500_000u128));
    }

    #[test]
    fn decimal_uint256_roundtrip() {
        // ETH roundtrip
        let original = dec!(123.456789012345678);
        let u256 = decimal_to_uint256(original, 18);
        let recovered = uint256_to_decimal(u256, 18);
        // Should be equal after truncation to 18 decimal places
        assert_eq!(recovered, dec!(123.456789012345678));

        // USDT roundtrip
        let original = dec!(999.99);
        let u256 = decimal_to_uint256(original, 6);
        let recovered = uint256_to_decimal(u256, 6);
        assert_eq!(recovered, dec!(999.99));
    }
}
