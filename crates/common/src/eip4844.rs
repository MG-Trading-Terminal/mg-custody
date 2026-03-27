//! EIP-4844 Blob Transactions (Proto-Danksharding) — L2 data availability.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// EIP-4844 blob transaction parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobTxParams {
    pub max_fee_per_blob_gas: Decimal,
    pub blob_versioned_hashes: Vec<String>,
    pub blob_count: u32,
    pub total_blob_size_bytes: u64,
}

/// Blob gas price tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobGasInfo {
    pub excess_blob_gas: u64,
    pub blob_gas_used: u64,
    pub blob_base_fee_wei: u64,
    pub blobs_per_block: u32,
    pub target_blobs_per_block: u32,
    pub max_blobs_per_block: u32,
}

/// Constants for EIP-4844.
pub const BLOB_SIZE: usize = 131_072; // 128 KB
pub const MAX_BLOBS_PER_TX: u32 = 6;
pub const TARGET_BLOBS_PER_BLOCK: u32 = 3;
pub const MAX_BLOBS_PER_BLOCK: u32 = 6;
pub const BLOB_TX_TYPE: u8 = 0x03;

/// Calculate blob base fee from excess blob gas.
pub fn calc_blob_base_fee(excess_blob_gas: u64) -> u64 {
    // Simplified: base_fee = e^(excess / 3338477)
    // Using integer approximation
    let min_fee: u64 = 1;
    if excess_blob_gas == 0 { return min_fee; }
    let factor = excess_blob_gas / 3_338_477;
    min_fee.saturating_mul(2u64.saturating_pow(factor as u32))
}

/// Estimate L2 posting cost via blobs.
pub fn estimate_l2_blob_cost(data_size_bytes: u64, blob_base_fee_wei: u64) -> BlobCostEstimate {
    let blobs_needed = ((data_size_bytes as f64) / (BLOB_SIZE as f64)).ceil() as u32;
    let blob_gas_per_blob: u64 = 131_072; // GAS_PER_BLOB
    let total_blob_gas = blob_gas_per_blob * blobs_needed as u64;
    let cost_wei = total_blob_gas * blob_base_fee_wei;

    BlobCostEstimate {
        blobs_needed,
        total_blob_gas,
        cost_wei,
        cost_gwei: cost_wei / 1_000_000_000,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobCostEstimate {
    pub blobs_needed: u32,
    pub total_blob_gas: u64,
    pub cost_wei: u64,
    pub cost_gwei: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_base_fee_zero_excess() {
        assert_eq!(calc_blob_base_fee(0), 1);
    }

    #[test]
    fn blob_base_fee_increases() {
        let fee_low = calc_blob_base_fee(1_000_000);
        let fee_high = calc_blob_base_fee(10_000_000);
        assert!(fee_high >= fee_low);
    }

    #[test]
    fn estimate_single_blob() {
        let est = estimate_l2_blob_cost(100_000, 1); // 100KB, 1 wei base fee
        assert_eq!(est.blobs_needed, 1);
    }

    #[test]
    fn estimate_multiple_blobs() {
        let est = estimate_l2_blob_cost(300_000, 1); // 300KB needs 3 blobs
        assert_eq!(est.blobs_needed, 3);
    }

    #[test]
    fn blob_tx_type_constant() {
        assert_eq!(BLOB_TX_TYPE, 0x03);
    }

    #[test]
    fn blob_size_128kb() {
        assert_eq!(BLOB_SIZE, 131_072);
    }
}
