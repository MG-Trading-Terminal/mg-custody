//! Solana fee estimation via JSON-RPC.

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use crate::FeeEstimate;
use custody_common::Chain;

pub const BASE_FEE_LAMPORTS: u64 = 5_000;

pub struct SolanaFeeEstimator {
    rpc_url: String,
    client: reqwest::Client,
}

impl SolanaFeeEstimator {
    pub fn new(rpc_url: &str) -> Self {
        Self { rpc_url: rpc_url.to_string(), client: reqwest::Client::new() }
    }

    pub async fn estimate(&self) -> Result<FeeEstimate> {
        // Query recent prioritization fees
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getRecentPrioritizationFees",
            "params": [[]] // empty account list = global
        });

        let resp: serde_json::Value = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send().await.context("Solana getRecentPrioritizationFees")?
            .json().await?;

        // Calculate median priority fee from recent slots
        let fees = resp["result"].as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f["prioritizationFee"].as_u64())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let priority_lamports = if fees.is_empty() {
            0
        } else {
            let mut sorted = fees;
            sorted.sort();
            sorted[sorted.len() / 2] // median
        };

        let base = Decimal::new(BASE_FEE_LAMPORTS as i64, 9);
        let priority = Decimal::new(priority_lamports as i64, 9);

        Ok(FeeEstimate {
            chain: Chain::Solana,
            base_fee: base,
            priority_fee: priority,
            max_fee: base + priority,
            estimated_at: chrono::Utc::now(),
        })
    }
}

pub fn estimate_fee_lamports(num_signatures: u32) -> u64 {
    BASE_FEE_LAMPORTS * num_signatures as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn base_fee() {
        assert_eq!(BASE_FEE_LAMPORTS, 5_000);
        assert_eq!(Decimal::new(5_000, 9), dec!(0.000005000));
    }

    #[test]
    fn multi_sig_fee() {
        assert_eq!(estimate_fee_lamports(1), 5_000);
        assert_eq!(estimate_fee_lamports(2), 10_000);
    }
}
