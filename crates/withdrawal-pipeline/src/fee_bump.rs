//! Fee bumping for stuck Ethereum transactions.
//!
//! If a transaction has been broadcast but not mined for N blocks,
//! re-sign with the same nonce but higher gas and rebroadcast.
//! EIP-1559 minimum replacement: +12.5% on max_fee_per_gas.

use custody_broadcaster::fee_bump_min;
use rust_decimal::Decimal;

/// Number of blocks to wait before considering a tx "stuck".
pub const STUCK_THRESHOLD_BLOCKS: u64 = 20;

/// Maximum number of fee bump attempts before giving up.
pub const MAX_BUMP_ATTEMPTS: u32 = 5;

/// Calculate the new gas price for a fee bump.
///
/// Returns the minimum replacement max_fee_per_gas (12.5% higher).
pub fn bumped_gas_price(current_max_fee: u128) -> u128 {
    fee_bump_min(current_max_fee)
}

/// Determine if a transaction should be bumped.
pub fn should_bump(
    blocks_since_broadcast: u64,
    bump_attempts: u32,
) -> bool {
    blocks_since_broadcast >= STUCK_THRESHOLD_BLOCKS && bump_attempts < MAX_BUMP_ATTEMPTS
}

/// Calculate the estimated cost increase from a fee bump (in gwei).
pub fn bump_cost_increase_gwei(current_gas_gwei: Decimal, gas_limit: u64) -> Decimal {
    let bumped = current_gas_gwei * Decimal::new(1125, 3); // * 1.125
    let increase = bumped - current_gas_gwei;
    increase * Decimal::new(gas_limit as i64, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn fee_bump_calculation() {
        // 50 gwei → at least 56.25 gwei
        let bumped = bumped_gas_price(50_000_000_000);
        assert!(bumped > 50_000_000_000);
        assert!(bumped >= 56_250_000_000); // 12.5% increase
    }

    #[test]
    fn should_bump_after_threshold() {
        assert!(!should_bump(19, 0)); // too early
        assert!(should_bump(20, 0));  // at threshold
        assert!(should_bump(50, 0));  // well past
    }

    #[test]
    fn should_not_bump_after_max_attempts() {
        assert!(!should_bump(100, 5)); // max attempts reached
        assert!(should_bump(100, 4));  // still ok
    }

    #[test]
    fn cost_increase_estimation() {
        // 50 gwei, 21000 gas limit → increase of ~6.25 gwei * 21000 = ~131250 gwei
        let increase = bump_cost_increase_gwei(dec!(50), 21_000);
        assert!(increase > dec!(0));
    }
}
