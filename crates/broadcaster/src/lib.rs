//! Transaction broadcast and rebroadcast.
//!
//! Submits signed transactions to blockchain nodes.
//! Monitors for inclusion and rebroadcasts if stuck.

pub mod bitcoin;
pub mod ethereum;
pub mod solana;
pub mod tron;

use custody_common::Chain;

/// Status of a broadcast transaction.
#[derive(Debug, Clone)]
pub struct TxStatus {
    pub block_number: u64,
    pub confirmations: u64,
    pub success: bool,
}

/// Trait for chain-specific transaction broadcasters.
pub trait Broadcaster: Send + Sync {
    fn chain(&self) -> Chain;

    fn broadcast(
        &self,
        signed_tx: &[u8],
    ) -> impl std::future::Future<Output = anyhow::Result<String>> + Send;

    fn tx_status(
        &self,
        tx_hash: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<Option<TxStatus>>> + Send;
}

/// Calculate the minimum replacement fee for EIP-1559 fee bumping.
///
/// The new max_fee_per_gas must be at least 12.5% higher than the original
/// (minimum bump to be accepted as a replacement transaction).
pub fn fee_bump_min(original_max_fee: u128) -> u128 {
    // 12.5% = multiply by 9, divide by 8 (exact integer math, rounds up)
    (original_max_fee * 9 + 7) / 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_bump_12_5_percent() {
        // 100 gwei → 112.5 gwei (rounds to 113)
        assert_eq!(fee_bump_min(100_000_000_000), 112_500_000_000);

        // 30 gwei → 33.75 gwei (exact: 30 * 9 / 8 = 33.75)
        assert_eq!(fee_bump_min(30_000_000_000), 33_750_000_000);

        // 0 → 0
        assert_eq!(fee_bump_min(0), 0);

        // 8 → 9 (exact 12.5%)
        assert_eq!(fee_bump_min(8), 9);
    }

    #[test]
    fn fee_bump_always_greater() {
        for fee in [1u128, 100, 1000, 1_000_000_000, 100_000_000_000] {
            assert!(fee_bump_min(fee) > fee, "Bump of {fee} should be greater");
        }
    }
}
