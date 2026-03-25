//! Bitcoin fee estimation via Bitcoin Core `estimatesmartfee` RPC.

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use crate::FeeEstimate;
use custody_common::Chain;

pub const DEFAULT_TARGET_BLOCKS: u32 = 6;
pub const MIN_FEE_RATE_SAT_PER_VB: u64 = 1;

pub struct BitcoinFeeEstimator {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    client: reqwest::Client,
    target_blocks: u32,
}

impl BitcoinFeeEstimator {
    pub fn new(rpc_url: &str, rpc_user: Option<String>, rpc_password: Option<String>, target_blocks: u32) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rpc_user, rpc_password,
            client: reqwest::Client::new(),
            target_blocks,
        }
    }

    pub async fn estimate(&self) -> Result<FeeEstimate> {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "estimatesmartfee",
            "params": [self.target_blocks]
        });

        let mut req = self.client.post(&self.rpc_url).json(&body);
        if let (Some(u), Some(p)) = (&self.rpc_user, &self.rpc_password) {
            req = req.basic_auth(u, Some(p));
        }

        let resp: serde_json::Value = req.send().await
            .context("Bitcoin estimatesmartfee RPC")?
            .json().await?;

        let feerate_btc_per_kb = resp["result"]["feerate"].as_f64();

        let sat_per_vb = match feerate_btc_per_kb {
            Some(rate) => Decimal::new(btc_per_kb_to_sat_per_vb(rate) as i64, 0),
            None => {
                tracing::warn!("No fee estimate from node, using minimum");
                Decimal::new(MIN_FEE_RATE_SAT_PER_VB as i64, 0)
            }
        };

        Ok(FeeEstimate {
            chain: Chain::Bitcoin,
            base_fee: sat_per_vb,
            priority_fee: Decimal::ZERO,
            max_fee: sat_per_vb,
            estimated_at: chrono::Utc::now(),
        })
    }
}

/// Convert BTC/kB to sat/vB. 1 BTC = 100M sats, 1 kB = 1000 vB.
pub fn btc_per_kb_to_sat_per_vb(btc_per_kb: f64) -> u64 {
    std::cmp::max((btc_per_kb * 100_000.0).round() as u64, MIN_FEE_RATE_SAT_PER_VB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_rate_conversion() {
        assert_eq!(btc_per_kb_to_sat_per_vb(0.0001), 10);
        assert_eq!(btc_per_kb_to_sat_per_vb(0.00001), 1);
        assert_eq!(btc_per_kb_to_sat_per_vb(0.001), 100);
        assert_eq!(btc_per_kb_to_sat_per_vb(0.0), 1); // minimum
    }
}
